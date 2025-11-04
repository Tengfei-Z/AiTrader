use ai_core::config::{AppConfig, DeepSeekConfig};
use anyhow::{anyhow, ensure, Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionNamedToolChoice, ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionToolArgs,
        ChatCompletionToolChoiceOption, ChatCompletionToolType, CreateChatCompletionRequestArgs,
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

pub const DEFAULT_FUNCTION_CALL_SYSTEM_PROMPT: &str = r#"你是一个专业的加密货币交易 AI，负责独立分析市场、制定交易计划并执行策略。你的目标是最大化风险调整后的收益（如 Sharpe Ratio），同时保障账户稳健运行。

工作职责：
1. 产出 Alpha：研判行情结构、识别交易机会、预测价格走向。
2. 决定仓位：合理分配资金、选择杠杆倍数、管理整体风险敞口。
3. 控制节奏：确定开仓与平仓时机，设置止盈止损。
4. 风险管理：避免过度暴露，确保有充足保证金与退出计划。

约束条件：
- 仅可交易白名单内的币种与合约。
- 杠杆上限 25X。
- 每个持仓必须具备完整的退出方案（止盈、止损、失效条件）。
- 输出需清晰、可审计，便于透明化展示。

可用 MCP 工具：
1. get_market_data：获取实时行情及技术指标
2. get_account_state：查询账户状态与持仓
3. execute_trade：执行交易（开/平仓）
4. update_exit_plan：更新已有仓位的退出计划

输出要求（每次响应）：
1. 思考总结（≤200 字）：概述市场状况、持仓状态、下一步计划。
2. 决策行动：如需操作，调用 MCP 工具并保证退出计划完整。
3. 置信度（0-100）：给出当前判断的信心水平。

策略提示：
- 风险优先，追求稳定的风险收益比。
- 避免无效频繁交易，关注成本。
- 保持严格止损，保护本金。
- 分散持仓，避免单一资产集中。
- 顺势而为，尊重趋势变化。
- 保持耐心，等待高质量信号。"#;

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
            .timeout(Duration::from_secs(20))  // HTTP 层面 20 秒总超时
            .connect_timeout(Duration::from_secs(10))  // 连接超时 10 秒
            .read_timeout(Duration::from_secs(15))  // 读取超时 15 秒（防止流式响应卡住）
            .pool_idle_timeout(Duration::from_secs(30))  // 连接池空闲超时
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

        for turn in 0..5 {  // 从8降到5，减少对话轮数
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
            let mut message_details = Vec::new();
            for (idx, msg) in messages.iter().enumerate() {
                let msg_json = serde_json::to_string(msg).unwrap_or_default();
                let char_count = msg_json.chars().count();
                total_chars += char_count;
                message_details.push(format!("msg[{}]: {} chars", idx, char_count));
            }
            let estimated_tokens = total_chars / 4; // 粗略估算：平均 4 字符 ≈ 1 token

            info!(
                function = %request.function,
                turn,
                model = %self.config.model,
                message_count = messages.len(),
                total_chars,
                estimated_tokens,
                message_breakdown = ?message_details,
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

            let mut assistant_builder = ChatCompletionRequestAssistantMessageArgs::default();
            if let Some(content) = choice.message.content.clone() {
                assistant_builder.content(content);
            }
            if let Some(tool_calls) = choice.message.tool_calls.clone() {
                assistant_builder.tool_calls(tool_calls);
            }
            let assistant_message = assistant_builder
                .build()
                .context("构建 assistant 消息失败")?;
            messages.push(assistant_message.into());

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

                    let tool_message = ChatCompletionRequestToolMessageArgs::default()
                        .tool_call_id(tool_call.id.clone())
                        .content(tool_content.clone())
                        .build()
                        .context("构建 tool 消息失败")?;
                    
                    info!(
                        tool_name = %tool_call.function.name,
                        turn,
                        tool_content_chars = tool_content.chars().count(),
                        "Tool message constructed, adding to conversation"
                    );
                    
                    messages.push(tool_message.into());
                }

                continue;
            }

            final_message = choice.message.content.clone();
            break;
        }

        if final_message.is_none() {
            warn!(
                function = %request.function,
                "DeepSeek conversation ended without final assistant message"
            );
        }

        let final_message_value = final_message.unwrap_or_default();

        info!(
            function = %request.function,
            "DeepSeek conversation completed"
        );

        let output = json!({
            "tool_results": tool_history,
            "final_message": final_message_value,
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
}

fn build_function_object(
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
            "get_market_data",
            "Fetch recent market metrics, indicators, and optional order book snapshots for specified coins.",
            json!({
                "type": "object",
                "properties": {
                    "coins": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["BTC", "ETH", "SOL", "BNB"] },
                        "minItems": 1
                    },
                    "timeframe": { "type": "string", "default": "3m" },
                    "quote": { "type": "string", "default": "USDT" },
                    "indicators": {
                        "type": "array",
                        "items": { "type": "string" },
                        "default": ["price"]
                    },
                    "include_orderbook": { "type": "boolean", "default": false },
                    "include_funding": { "type": "boolean", "default": false },
                    "include_open_interest": { "type": "boolean", "default": false },
                    "simulated_trading": { "type": "boolean", "default": true }
                },
                "required": ["coins"],
                "additionalProperties": false
            }),
        ),
        (
            "get_account_state",
            "Aggregate OKX account balances, active positions, and performance metrics.",
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
            "Place or close a leveraged trade on OKX using either live or simulated credentials.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["open_long", "open_short", "close_position"] },
                    "coin": { "type": "string", "enum": ["BTC", "ETH", "SOL", "BNB"] },
                    "instrument_id": { "type": "string" },
                    "instrument_type": { "type": "string" },
                    "quote": { "type": "string", "default": "USDT" },
                    "td_mode": { "type": "string", "default": "cross" },
                    "margin_currency": { "type": "string" },
                    "leverage": { "type": "number" },
                    "margin_amount": { "type": "number" },
                    "quantity": { "type": "number" },
                    "position_id": { "type": "string" },
                    "exit_plan": { "type": "object" },
                    "confidence": { "type": "integer" },
                    "simulated_trading": { "type": "boolean", "default": true }
                },
                "required": ["action", "coin", "simulated_trading"],
                "additionalProperties": true
            }),
        ),
        (
            "update_exit_plan",
            "Adjust take-profit and stop-loss parameters for an existing position.",
            json!({
                "type": "object",
                "properties": {
                    "position_id": { "type": "string" },
                    "new_profit_target": { "type": "number" },
                    "new_stop_loss": { "type": "number" },
                    "new_invalidation": { "type": "string" },
                    "instrument_id": { "type": "string" },
                    "td_mode": { "type": "string" },
                    "simulated_trading": { "type": "boolean", "default": true }
                },
                "required": ["position_id", "simulated_trading"],
                "additionalProperties": false
            }),
        ),
    ]
}
