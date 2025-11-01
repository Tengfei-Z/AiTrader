use ai_core::config::{AppConfig, DeepSeekConfig};
use anyhow::{anyhow, Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionNamedToolChoice, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionToolArgs,
        ChatCompletionToolChoiceOption, ChatCompletionToolType, CreateChatCompletionRequestArgs,
        FunctionName, FunctionObjectArgs,
    },
    Client as OpenAIClient,
};
use async_trait::async_trait;
use tracing::instrument;

use crate::schema::{FunctionCallRequest, FunctionCallResponse};
use mcp_adapter::account::{fetch_account_state, AccountStateRequest};
use okx::OkxRestClient;
use serde_json::{self, json, Value};

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
        let openai_config = OpenAIConfig::new()
            .with_api_key(config.api_key.clone())
            .with_api_base(config.endpoint.trim_end_matches('/').to_string());

        Ok(Self {
            client: OpenAIClient::with_config(openai_config),
            config,
            app_config: None,
        })
    }
}

#[async_trait]
impl FunctionCaller for DeepSeekClient {
    #[instrument(skip(self, request), fields(model = %self.config.model))]
    async fn call_function(&self, request: FunctionCallRequest) -> Result<FunctionCallResponse> {
        let system_prompt = request
            .metadata
            .get("system_prompt")
            .and_then(|v| v.as_str())
            .unwrap_or(
                "You are a function calling assistant. Always respond by calling the requested tool with the provided JSON arguments.",
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

        let mut function_builder = FunctionObjectArgs::default();
        function_builder.name(request.function.clone());
        if let Some(desc) = function_description {
            function_builder.description(desc);
        }
        function_builder.parameters(Some(parameters_schema));
        let function_object = function_builder.build().context("构建函数描述失败")?;

        let tool = ChatCompletionToolArgs::default()
            .function(function_object)
            .build()
            .context("构建工具描述失败")?;

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

        let chat_request = CreateChatCompletionRequestArgs::default()
            .model(self.config.model.clone())
            .messages([system_message.into(), user_message.into()])
            .tools(vec![tool])
            .tool_choice(ChatCompletionToolChoiceOption::Named(
                ChatCompletionNamedToolChoice {
                    r#type: ChatCompletionToolType::Function,
                    function: FunctionName {
                        name: request.function.clone(),
                    },
                },
            ))
            .temperature(0_f32)
            .build()
            .context("构建 ChatCompletion 请求失败")?;

        let response = self
            .client
            .chat()
            .create(chat_request)
            .await
            .context("调用 DeepSeek Chat 接口失败")?;

        let choice = response
            .choices
            .first()
            .ok_or_else(|| anyhow!("DeepSeek 返回结果为空"))?;

        let usage_value = response
            .usage
            .as_ref()
            .and_then(|u| serde_json::to_value(u).ok());

        if let Some(tool_calls) = &choice.message.tool_calls {
            if let Some(tool_call) = tool_calls.first() {
                let arguments_raw = tool_call.function.arguments.clone();
                let parsed_arguments: Value = serde_json::from_str(&arguments_raw)
                    .unwrap_or_else(|_| Value::String(arguments_raw.clone()));

                if let Some(execution) = self
                    .execute_local_tool(&tool_call.function.name, &parsed_arguments)
                    .await?
                {
                    return Ok(FunctionCallResponse {
                        output: execution,
                        usage: usage_value,
                        message: choice.message.content.clone(),
                    });
                }

                let output = json!({
                    "tool_call": {
                        "id": tool_call.id,
                        "name": tool_call.function.name,
                        "arguments": parsed_arguments,
                    }
                });

                return Ok(FunctionCallResponse {
                    output,
                    usage: usage_value,
                    message: choice.message.content.clone(),
                });
            }
        }

        let content = choice.message.content.clone().unwrap_or_default();

        Ok(FunctionCallResponse {
            output: json!({ "content": content }),
            usage: usage_value,
            message: choice.message.content.clone(),
        })
    }
}

impl DeepSeekClient {
    async fn execute_local_tool(&self, name: &str, arguments: &Value) -> Result<Option<Value>> {
        match name {
            "get_account_state" => {
                let app_config = match &self.app_config {
                    Some(cfg) => cfg,
                    None => return Ok(None),
                };

                let okx_client =
                    OkxRestClient::from_config(app_config).context("初始化 OKX 客户端失败")?;

                let request: AccountStateRequest =
                    serde_json::from_value(arguments.clone()).unwrap_or_default();

                let account_state = fetch_account_state(&okx_client, &request)
                    .await
                    .context("执行本地账户聚合失败")?;

                let value = serde_json::to_value(account_state).context("序列化账户结果失败")?;

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

        response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .ok_or_else(|| anyhow!("DeepSeek Chat 返回结果为空"))
    }
}
