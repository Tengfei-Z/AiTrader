use ai_core::config::{AppConfig, DeepSeekConfig};
use anyhow::{anyhow, ensure, Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionNamedToolChoice,
        ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs,
        ChatCompletionTool, ChatCompletionToolArgs,
        ChatCompletionToolChoiceOption, ChatCompletionToolType,
        CreateChatCompletionRequestArgs,
        FunctionName, FunctionObject, FunctionObjectArgs,
    },
    Client as OpenAIClient,
};
use async_trait::async_trait;
use std::time::Duration;
use tracing::{info, instrument, warn};

use crate::schema::{FunctionCallRequest, FunctionCallResponse};
use mcp_adapter::{
    account::{fetch_account_state, AccountStateRequest},
    market::{fetch_market_data, MarketDataRequest},
    trade::{
        execute_trade as execute_trade_tool, update_exit_plan, ExecuteTradeRequest,
        UpdateExitPlanRequest,
    },
};
use okx::OkxRestClient;
use serde_json::{self, json, Value};

const ALLOWED_COINS: &[&str] = &["BTC", "ETH", "SOL", "BNB"];

pub const DEFAULT_FUNCTION_CALL_SYSTEM_PROMPT: &str = r#"你是一个专业的加密货币交易助手，严格遵循两步工作流程。

工具说明：
1. get_account_state：
   参数要求：{
     "include_history": true,
     "include_performance": true,
     "include_positions": true,
     "simulated_trading": true
   }
   用途：仅用于首次获取账户状态

2. execute_trade：
   参数格式：{
     "instrument_id": "BTC-USDT-SWAP",
     "direction": "buy"/"sell",
     "size": <number>,
     "take_profit": <number>,
     "stop_loss": <number>
   }
   用途：执行交易决策

强制执行流程：
1. 第一步：调用 get_account_state（仅限一次）
2. 第二步：分析账户状态后必须调用 execute_trade

交易规则：
- 合约品种：BTC-USDT-SWAP
- 仓位控制：≤账户30%
- 止损范围：2-3%
- 禁止重复查询账户
- 必须完成交易决策

回复要求：
1. 分析：说明当前账户状态
2. 决策：给出具体交易行动
3. 执行：调用 execute_trade

输出格式：
1. 操作结果：简述本次操作的执行结果
2. 账户信息：显示关键的账户数据
3. 注意事项：如有风险提示则展示"#;

#[async_trait]
pub trait FunctionCaller: Send + Sync {
    async fn call_function(&self, request: FunctionCallRequest) -> Result<FunctionCallResponse>;
}

#[derive(Debug, Clone)]
pub struct DeepSeekClient {
    client: OpenAIClient<OpenAIConfig>,
    config: DeepSeekConfig,
    app_config: Option<AppConfig>,
}

impl DeepSeekClient {
    pub fn from_app_config(config: &AppConfig) -> Result<Self> {
        let deepseek = config.require_deepseek_config()?.clone();
        let mut client = Self::new(deepseek)?;
        client.app_config = Some(config.clone());
        Ok(client)
    }

    pub fn new(config: DeepSeekConfig) -> Result<Self> {
        // 创建自定义 reqwest client，设置 HTTP 超时
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))  // HTTP 层面 15 秒总超时，与 API 调用超时一致
            .connect_timeout(Duration::from_secs(5))  // 连接超时 5 秒
            .read_timeout(Duration::from_secs(10))  // 读取超时 10 秒（降低流式响应卡住的风险）
            .pool_idle_timeout(Duration::from_secs(20))  // 连接池空闲超时，降低资源占用
            .tcp_nodelay(true)  // 启用 TCP_NODELAY，减少延迟
            .build()
            .context("创建 HTTP 客户端失败")?;

        let openai_config = OpenAIConfig::new()
            .with_api_key(config.api_key.clone())
            .with_api_base(config.endpoint.trim_end_matches('/').to_string());

        Ok(Self {
            client: OpenAIClient::with_config(openai_config).with_http_client(http_client),
            config,
            app_config: None,
        })
    }
}

#[async_trait]
impl FunctionCaller for DeepSeekClient {
    #[instrument(skip(self, request), fields(model = %self.config.model))]
    async fn call_function(&self, request: FunctionCallRequest) -> Result<FunctionCallResponse> {
        info!(
            function = %request.function,
            arguments = %request.arguments,
            metadata = %request.metadata,
            "Preparing DeepSeek function call"
        );

        let system_prompt = request
            .metadata
            .get("system_prompt")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_FUNCTION_CALL_SYSTEM_PROMPT);

        info!(
            function = %request.function,
            system_prompt_preview = %truncate_for_log(system_prompt, 240),
            "Using system prompt for DeepSeek request"
        );

        let function_description = request
            .metadata
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let parameters_schema = request
            .metadata
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "type": "object",
                    "additionalProperties": true
                })
            });

        let tool_catalog = build_tool_catalog(
            &request.function,
            function_description.as_deref(),
            &parameters_schema,
        )?;

        let system_message = ChatCompletionRequestSystemMessageArgs::default()
            .content(system_prompt)
            .build()
            .context("构建 system 消息失败")?;

        let user_payload = json!({
            "function": request.function,
            "arguments": request.arguments,
            "metadata": request.metadata,
        });

        let user_message = ChatCompletionRequestUserMessageArgs::default()
            .content(serde_json::to_string(&user_payload).unwrap_or_default())
            .build()
            .context("构建 user 消息失败")?;

        let mut messages = vec![system_message.into(), user_message.into()];
        let mut tool_history: Vec<Value> = Vec::new();
        let mut usage_log: Vec<Value> = Vec::new();
        let mut final_message: Option<String> = None;
        let mut force_tool_choice = true;
        let mut final_turn = 0;

        for turn in 0..5 {  // 从8降到5，减少对话轮数
            final_turn = turn;
            info!(
                function = %request.function,
                turn,
                total_messages = messages.len(),
                tool_history_count = tool_history.len(),
                "Starting conversation turn"
            );

            let chat_tools = build_chat_tools(&tool_catalog)?;
            let mut request_builder = CreateChatCompletionRequestArgs::default();
            request_builder
                .model(self.config.model.clone())
                .messages(messages.clone())
                .tools(chat_tools)
                .temperature(0_f32);

            if force_tool_choice {
                request_builder.tool_choice(ChatCompletionToolChoiceOption::Named(
                    ChatCompletionNamedToolChoice {
                        r#type: ChatCompletionToolType::Function,
                        function: FunctionName {
                            name: request.function.clone(),
                        },
                    },
                ));
            }

            let chat_request = request_builder
                .build()
                .context("构建 ChatCompletion 请求失败")?;

            force_tool_choice = false;

            // 计算并记录发送给 DeepSeek 的消息统计
            let mut total_chars = 0;
            let mut total_tools = 0;
            let mut message_details = Vec::new();
            let mut message_types = Vec::new();

            for (idx, msg) in messages.iter().enumerate() {
                let msg_json = serde_json::to_string(msg).unwrap_or_default();
                let char_count = msg_json.chars().count();
                total_chars += char_count;

                // 分析消息类型和内容
                let msg_type = if msg_json.contains("\"role\":\"system\"") {
                    "system"
                } else if msg_json.contains("\"role\":\"user\"") {
                    "user"
                } else if msg_json.contains("\"role\":\"assistant\"") {
                    "assistant"
                } else if msg_json.contains("\"role\":\"tool\"") {
                    total_tools += 1;
                    "tool"
                } else {
                    "unknown"
                };

                message_types.push(msg_type);
                message_details.push(format!("msg[{}]: {} chars ({})", idx, char_count, msg_type));
            }

            let estimated_tokens = total_chars / 4; // 粗略估算：平均 4 字符 ≈ 1 token
            
            info!(
                function = %request.function,
                turn,
                model = %self.config.model,
                message_count = messages.len(),
                total_chars,
                estimated_tokens,
                total_tools,
                message_types = ?message_types,
                message_details = ?message_details,
                "Sending DeepSeek chat completion request"
            );

            // Set a 15 second timeout for the API call
            let timeout_duration = Duration::from_secs(15);
            
            let start_time = std::time::Instant::now();
            
            info!(
                function = %request.function,
                turn,
                timeout_secs = 15,
                "About to call DeepSeek API with timeout"
            );
            
            let response = match tokio::time::timeout(
                timeout_duration, 
                self.client.chat().create(chat_request)
            ).await {
                Ok(result) => match result {
                    Ok(resp) => {
                        let elapsed = start_time.elapsed();
                        info!(
                            function = %request.function,
                            turn,
                            elapsed_secs = elapsed.as_secs_f64(),
                            "Successfully received response from DeepSeek API"
                        );
                        resp
                    }
                    Err(e) => {
                        let elapsed = start_time.elapsed();
                        warn!(
                            function = %request.function,
                            turn,
                            elapsed_secs = elapsed.as_secs_f64(),
                            error = %e,
                            "Failed to call DeepSeek Chat API"
                        );
                        return Err(e).context("调用 DeepSeek Chat 接口失败");
                    }
                },
                Err(_) => {
                    let elapsed = start_time.elapsed();
                    warn!(
                        function = %request.function,
                        turn,
                        timeout_secs = 15,
                        elapsed_secs = elapsed.as_secs_f64(),
                        message_count = messages.len(),
                        "DeepSeek API call timed out after waiting"
                    );
                    return Err(anyhow!("DeepSeek API 调用超时（15秒）"));
                }
            };

            if let Some(usage) = response.usage.as_ref() {
                if let Ok(value) = serde_json::to_value(usage) {
                    usage_log.push(value);
                }
            }

            let choice = response
                .choices
                .first()
                .ok_or_else(|| anyhow!("DeepSeek 返回结果为空"))?;

            info!(
                function = %request.function,
                turn,
                response_message = ?choice.message,
                "Received DeepSeek response"
            );

            // 优化消息处理
            if let Some(tool_calls) = &choice.message.tool_calls {
                // 如果即将超过最大轮数，拒绝继续执行工具
                if turn >= 4 {
                    warn!(
                        function = %request.function,
                        turn,
                        tool_calls_count = tool_calls.len(),
                        "Reached maximum turns, ignoring tool calls and forcing completion"
                    );
                    final_message = Some(format!(
                        "已达到最大对话轮数（{}），无法继续执行工具调用。当前工具历史：{:?}",
                        turn + 1,
                        tool_history
                    ));
                    break;
                }
                
                // 记录当前工具调用数量
                let tool_count = tool_calls.len();
                if tool_count > 1 {
                    warn!(
                        function = %request.function,
                        turn,
                        tool_count,
                        "Multiple tool calls in single turn"
                    );
                }

                for tool_call in tool_calls {
                    let arguments_raw = tool_call.function.arguments.clone();
                    let parsed_arguments: Value = serde_json::from_str(&arguments_raw)
                        .unwrap_or_else(|_| Value::String(arguments_raw.clone()));

                    info!(
                        tool_name = %tool_call.function.name,
                        tool_arguments = %parsed_arguments,
                        tool_call_id = %tool_call.id,
                        turn,
                        "DeepSeek suggested tool invocation"
                    );

                    let execution = self
                        .execute_local_tool(&tool_call.function.name, &parsed_arguments)
                        .await?;

                    let Some(result) = execution else {
                        warn!(
                            tool_name = %tool_call.function.name,
                            "No local executor found for suggested tool, returning payload"
                        );
                        let output = json!({
                            "tool_call": {
                                "id": tool_call.id,
                                "name": tool_call.function.name,
                                "arguments": parsed_arguments,
                            }
                        });
                        return Ok(FunctionCallResponse {
                            output,
                            usage: if usage_log.is_empty() {
                                None
                            } else {
                                Some(Value::Array(usage_log))
                            },
                            message: choice.message.content.clone(),
                        });
                    };

                    info!(
                        tool_name = %tool_call.function.name,
                        turn,
                        "Executed local tool for DeepSeek request"
                    );

                    let tool_content = serde_json::to_string(&result).unwrap_or_default();

                    info!(
                        tool_name = %tool_call.function.name,
                        turn,
                        tool_output_size_bytes = tool_content.len(),
                        tool_output_preview = %truncate_for_log(&tool_content, 240),
                        "Local tool execution completed"
                    );

                    let record = json!({
                        "id": tool_call.id,
                        "name": tool_call.function.name,
                        "arguments": parsed_arguments,
                        "output": result
                    });
                    tool_history.push(record);

                    // 优化结果消息格式
                    // 先计算字符数以供日志使用
                    let content_length = tool_content.chars().count();

                    // 构造消息内容
                    let (msg_prefix, msg_content) = match tool_call.function.name.as_str() {
                        "get_market_data" => {
                            // 对市场数据做特殊处理，突出显示关键信息
                            let value: Value = serde_json::from_str(&tool_content).unwrap_or_default();
                            let coins = value.get("coins").and_then(|v| v.as_object());
                            if let Some(coin_data) = coins {
                                let mut summary = Vec::new();
                                for (coin, data) in coin_data {
                                    if let Some(data_obj) = data.as_object() {
                                        let price = data_obj.get("current_price")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or_default();
                                        let funding = data_obj.get("funding_rate")
                                            .and_then(|v| v.as_f64())
                                            .map(|f| format!("资金费率:{:.4}%", f * 100.0))
                                            .unwrap_or_default();
                                        summary.push(format!("{}=${:.2} {}", coin, price, funding));
                                    }
                                }
                                ("市场概况", summary.join(", "))
                            } else {
                                ("市场数据", tool_content.clone())
                            }
                        }
                        "get_account_state" => ("账户状态", tool_content.clone()),
                        "execute_trade" => ("交易执行", tool_content.clone()),
                        "update_exit_plan" => ("退出计划", tool_content.clone()),
                        _ => ("工具执行结果", tool_content.clone())
                    };
                    
                    let user_message = ChatCompletionRequestUserMessageArgs::default()
                        .content(format!("{}：{}", msg_prefix, msg_content))
                        .build()
                        .context("构建数据消息失败")?;
                    
                    info!(
                        tool_name = %tool_call.function.name,
                        turn,
                        msg_prefix = %msg_prefix,
                        content_chars = content_length,
                        "Data message constructed, adding to conversation"
                    );
                    
                    messages.push(user_message.into());
                }

                continue;
            }

            // 检查是否有有效的响应内容
            if let Some(content) = &choice.message.content {
                if !content.trim().is_empty() {
                    final_message = Some(content.clone());
                    break;
                }
                // 如果内容为空且是工具调用，记录警告
                if choice.message.tool_calls.is_some() {
                    warn!(
                        function = %request.function,
                        turn,
                        "Empty response with tool call, requiring explanation"
                    );
                    // 加入提醒消息
                    let reminder = ChatCompletionRequestUserMessageArgs::default()
                        .content("请提供具体的分析和决策说明，不要重复调用相同的工具。每次工具调用都必须有明确的目的和解释。")
                        .build()
                        .context("构建提醒消息失败")?;
                    messages.push(reminder.into());
                    continue;
                }
            }
            final_message = choice.message.content.clone();
            break;
        }

        if final_message.is_none() || final_message.as_ref().map_or(true, |s| s.trim().is_empty()) {
            warn!(
                function = %request.function,
                "DeepSeek conversation ended without valid assistant message"
            );
            // 如果没有有效回复，返回一个错误信息
            final_message = Some("错误：模型未能提供有效的分析和决策说明。请重试。".to_string());
        }

        let final_message_value = final_message.unwrap_or_default();

        info!(
            function = %request.function,
            "DeepSeek conversation completed"
        );

        let output = json!({
            "tool_results": tool_history,
            "final_message": final_message_value,
            "execution_info": {
                "turns_completed": final_turn + 1,
                "messages_exchanged": messages.len(),
                "tool_calls_made": tool_history.len()
            }
        });

        Ok(FunctionCallResponse {
            output,
            usage: if usage_log.is_empty() {
                None
            } else {
                Some(Value::Array(usage_log))
            },
            message: Some(final_message_value),
        })
    }
}

impl DeepSeekClient {
    async fn execute_local_tool(&self, name: &str, arguments: &Value) -> Result<Option<Value>> {
        match name {
            "get_account_state" => {
                let mut request: AccountStateRequest =
                    serde_json::from_value(arguments.clone()).unwrap_or_default();

                info!(
                    tool = name,
                    arguments = %arguments,
                    "Executing local tool handler"
                );

                enforce_simulated(&mut request.simulated_trading);

                let app_config = get_app_config(&self.app_config)?;

                let okx_client = OkxRestClient::from_config_simulated(app_config)
                    .context("初始化 OKX 客户端失败")?;

                let account_state = fetch_account_state(&okx_client, &request)
                    .await
                    .context("执行本地账户聚合失败")?;

                let value = serde_json::to_value(account_state).context("序列化账户结果失败")?;

                info!(tool = name, "Local tool completed successfully");

                Ok(Some(value))
            }
            "get_market_data" => {
                let mut request: MarketDataRequest =
                    serde_json::from_value(arguments.clone()).unwrap_or_default();

                info!(
                    tool = name,
                    simulated = request.simulated_trading,
                    coins = ?request.coins,
                    "Executing local tool handler"
                );

                enforce_simulated(&mut request.simulated_trading);
                request.coins = sanitize_coins(request.coins)?;

                let app_config = get_app_config(&self.app_config)?;

                let okx_client = OkxRestClient::from_config_simulated(app_config)
                    .context("初始化 OKX 客户端失败")?;

                let response = match fetch_market_data(&okx_client, &request).await {
                    Ok(resp) => resp,
                    Err(err) => {
                        warn!(
                            tool = name,
                            error = ?err,
                            coins = ?request.coins,
                            "拉取行情数据失败"
                        );
                        return Ok(Some(json!({
                            "error": format!("fetch_market_data failed: {err}")
                        })));
                    }
                };

                let value = serde_json::to_value(response).context("序列化行情结果失败")?;

                info!(tool = name, "Local tool completed successfully");

                Ok(Some(value))
            }
            "execute_trade" => {
                let mut request: ExecuteTradeRequest =
                    serde_json::from_value(arguments.clone()).unwrap_or_default();

                info!(
                    tool = name,
                    action = ?request.action,
                    coin = %request.coin,
                    simulated = request.simulated_trading,
                    "Executing local tool handler"
                );

                enforce_simulated(&mut request.simulated_trading);
                ensure_allowed_coin(&request.coin)?;
                if let Some(inst) = request.instrument_id.as_ref() {
                    ensure_allowed_instrument(inst)?;
                }

                let app_config = get_app_config(&self.app_config)?;

                let okx_client = OkxRestClient::from_config_simulated(app_config)
                    .context("初始化 OKX 客户端失败")?;

                let response = match execute_trade_tool(&okx_client, &request).await {
                    Ok(resp) => resp,
                    Err(err) => {
                        warn!(
                            tool = name,
                            error = ?err,
                            action = ?request.action,
                            coin = %request.coin,
                            "执行交易失败"
                        );
                        return Ok(Some(json!({
                            "error": format!("execute_trade failed: {err}")
                        })));
                    }
                };

                let value = serde_json::to_value(response).context("序列化交易结果失败")?;

                info!(tool = name, "Local tool completed successfully");

                Ok(Some(value))
            }
            "update_exit_plan" => {
                let mut request: UpdateExitPlanRequest =
                    serde_json::from_value(arguments.clone()).unwrap_or_default();

                info!(
                    tool = name,
                    position_id = %request.position_id,
                    simulated = request.simulated_trading,
                    "Executing local tool handler"
                );

                enforce_simulated(&mut request.simulated_trading);

                let app_config = get_app_config(&self.app_config)?;

                let okx_client = OkxRestClient::from_config_simulated(app_config)
                    .context("初始化 OKX 客户端失败")?;

                let response = match update_exit_plan(&okx_client, &request).await {
                    Ok(resp) => resp,
                    Err(err) => {
                        warn!(
                            tool = name,
                            error = ?err,
                            position_id = %request.position_id,
                            "更新退出计划失败"
                        );
                        return Ok(Some(json!({
                            "error": format!("update_exit_plan failed: {err}")
                        })));
                    }
                };

                let value = serde_json::to_value(response).context("序列化退出计划结果失败")?;

                info!(tool = name, "Local tool completed successfully");

                Ok(Some(value))
            }
            _ => Ok(None),
        }
    }

    #[instrument(skip(self, prompt), fields(model = %self.config.model))]
    pub async fn chat_completion(&self, prompt: &str) -> Result<String> {
        let system_message = ChatCompletionRequestSystemMessageArgs::default()
            .content("You are a helpful assistant.")
            .build()
            .context("构建 system 消息失败")?;

        let user_message = ChatCompletionRequestUserMessageArgs::default()
            .content(prompt)
            .build()
            .context("构建 user 消息失败")?;

        let chat_request = CreateChatCompletionRequestArgs::default()
            .model(self.config.model.clone())
            .messages([system_message.into(), user_message.into()])
            .build()
            .context("构建 ChatCompletion 请求失败")?;

        let response = self
            .client
            .chat()
            .create(chat_request)
            .await
            .context("调用 DeepSeek Chat 接口失败")?;

        info!(
            model = %self.config.model,
            prompt = %truncate_for_log(prompt, 240),
            "DeepSeek chat completion request sent"
        );

        response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .ok_or_else(|| anyhow!("DeepSeek Chat 返回结果为空"))
    }
}

fn truncate_for_log(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    text.chars().take(max_chars).collect::<String>() + "…"
}

fn enforce_simulated(flag: &mut bool) {
    if !*flag {
        warn!("非模拟调用已被禁用，自动切换到模拟账户");
        *flag = true;
    }
}

fn sanitize_coins(coins: Vec<String>) -> Result<Vec<String>> {
    let mut filtered: Vec<String> = Vec::new();

    for coin in coins {
        let upper = coin.to_ascii_uppercase();
        if ALLOWED_COINS.contains(&upper.as_str()) {
            if !filtered.contains(&upper) {
                filtered.push(upper);
            }
        } else {
            warn!(coin = %coin, "币种不在允许列表，已忽略");
        }
    }

    ensure!(
        !filtered.is_empty(),
        "请求币种均不在允许列表 {:?}",
        ALLOWED_COINS
    );

    Ok(filtered)
}

fn ensure_allowed_coin(coin: &str) -> Result<()> {
    let upper = coin.to_ascii_uppercase();
    ensure!(
        ALLOWED_COINS.contains(&upper.as_str()),
        "币种 {} 不在允许列表 {:?}",
        coin,
        ALLOWED_COINS
    );
    Ok(())
}

fn ensure_allowed_instrument(instrument_id: &str) -> Result<()> {
    let coin = instrument_id.split('-').next().unwrap_or(instrument_id);
    ensure_allowed_coin(coin)
}

fn get_app_config(app_config: &Option<AppConfig>) -> Result<&AppConfig> {
    app_config
        .as_ref()
        .ok_or_else(|| anyhow!("AppConfig 未初始化，无法执行本地工具"))
}

fn build_tool_catalog(
    primary_name: &str,
    primary_description: Option<&str>,
    primary_schema: &Value,
) -> Result<Vec<FunctionObject>> {
    let mut tools = Vec::new();

    tools.push(build_function_object(
        primary_name,
        primary_description,
        Some(primary_schema.clone()),
    )?);

                    // 针对每个时间周期生成工具定义
                    for (name, description, schema) in default_tool_definitions() {
                        if name == primary_name {
                            continue;
                        }
                        tools.push(build_function_object(
                            name,
                            Some(description),
                            Some(schema),
                        )?);
                    }

                    Ok(tools)
                }fn build_function_object(
    name: &str,
    description: Option<&str>,
    parameters: Option<Value>,
) -> Result<FunctionObject> {
    let mut builder = FunctionObjectArgs::default();
    builder.name(name.to_string());
    if let Some(desc) = description {
        builder.description(desc.to_string());
    }
    if let Some(schema) = parameters {
        builder.parameters(Some(schema));
    }
    builder.build().context("构建函数描述失败")
}

fn build_chat_tools(tools: &[FunctionObject]) -> Result<Vec<ChatCompletionTool>> {
    tools
        .iter()
        .map(|tool| {
            ChatCompletionToolArgs::default()
                .function(tool.clone())
                .build()
                .context("构建工具描述失败")
        })
        .collect()
}

fn default_tool_definitions() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "get_account_state",
            "查询账户余额、当前持仓和交易历史。",
            json!({
                "type": "object",
                "properties": {
                    "include_positions": { "type": "boolean", "default": true },
                    "include_history": { "type": "boolean", "default": true },
                    "include_performance": { "type": "boolean", "default": true },
                    "simulated_trading": { "type": "boolean", "default": true }
                },
                "required": [
                    "include_positions",
                    "include_history",
                    "include_performance",
                    "simulated_trading"
                ],
                "additionalProperties": false
            }),
        ),
        (
            "execute_trade",
            "执行交易操作，包括开仓或平仓 BTC 永续合约。",
            json!({
                "type": "object",
                "properties": {
                    "action": { 
                        "type": "string", 
                        "enum": ["open_long", "open_short", "close_position"],
                        "description": "交易动作：开多、开空或平仓"
                    },
                    "instrument_id": { 
                        "type": "string",
                        "default": "BTC-USDT-SWAP",
                        "description": "合约 ID，默认 BTC 永续"
                    },
                    "quantity": { 
                        "type": "number",
                        "minimum": 0,
                        "description": "交易数量" 
                    },
                    "position_id": { 
                        "type": "string",
                        "description": "平仓时需要提供的持仓 ID" 
                    },
                    "leverage": { 
                        "type": "number", 
                        "minimum": 1,
                        "maximum": 25,
                        "default": 10,
                        "description": "杠杆倍数，1-25倍"
                    },
                    "simulated_trading": { 
                        "type": "boolean", 
                        "default": true,
                        "description": "是否使用模拟账户"
                    }
                },
                "required": ["action", "simulated_trading"],
                "additionalProperties": false,
                "allOf": [
                    {
                        "if": {
                            "properties": { "action": { "enum": ["open_long", "open_short"] } }
                        },
                        "then": {
                            "required": ["quantity", "leverage"]
                        }
                    },
                    {
                        "if": {
                            "properties": { "action": { "const": "close_position" } }
                        },
                        "then": {
                            "required": ["position_id"]
                        }
                    }
                ]
            }),
        ),
    ]
}
