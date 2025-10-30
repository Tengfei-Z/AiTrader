use ai_core::config::{AppConfig, DeepSeekConfig};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::{Client, Method, StatusCode};
use tracing::instrument;

use crate::schema::{
    ApiRequest, ChatMessage, ChatRequest, ChatResponse, FunctionCallRequest, FunctionCallResponse,
};

#[async_trait]
pub trait FunctionCaller: Send + Sync {
    async fn call_function(&self, request: FunctionCallRequest) -> Result<FunctionCallResponse>;
}

#[derive(Debug, Clone)]
pub struct DeepSeekClient {
    http: Client,
    config: DeepSeekConfig,
}

impl DeepSeekClient {
    pub fn from_app_config(config: &AppConfig) -> Result<Self> {
        let deepseek = config.require_deepseek_config()?.clone();
        Self::new(deepseek)
    }

    pub fn new(config: DeepSeekConfig) -> Result<Self> {
        let http = Client::new();
        Ok(Self { http, config })
    }
}

#[async_trait]
impl FunctionCaller for DeepSeekClient {
    #[instrument(skip(self, request), fields(model = %self.config.model))]
    async fn call_function(&self, request: FunctionCallRequest) -> Result<FunctionCallResponse> {
        let url = format!("{}/v1/function-call", self.config.endpoint.trim_end_matches('/'));
        let payload = ApiRequest {
            model: &self.config.model,
            function_call: &request,
        };

        let response = self
            .http
            .request(Method::POST, url)
            .bearer_auth(&self.config.api_key)
            .json(&payload)
            .send()
            .await?;

        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow!(
                "DeepSeek 调用失败：认证失败，请检查 DEEPSEEK_API_KEY"
            ));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "DeepSeek 调用失败：状态码 {}，返回内容：{}",
                status,
                body
            ));
        }

        let raw: serde_json::Value = response
            .json()
            .await
            .context("DeepSeek 返回内容解析失败")?;

        if let Some(data) = raw.get("data") {
            let parsed: FunctionCallResponse = serde_json::from_value(data.clone())
                .context("DeepSeek data 字段解析失败")?;
            Ok(parsed)
        } else {
            Err(anyhow!(
                "DeepSeek 返回结果不包含 data 字段：{}",
                raw
            ))
        }
    }
}

impl DeepSeekClient {
    #[instrument(skip(self, prompt), fields(model = %self.config.model))]
    pub async fn chat_completion(&self, prompt: &str) -> Result<String> {
        let url = format!(
            "{}/v1/chat/completions",
            self.config.endpoint.trim_end_matches('/')
        );

        let request = ChatRequest {
            model: &self.config.model,
            messages: vec![ChatMessage {
                role: "user",
                content: prompt.to_string(),
            }],
        };

        let response = self
            .http
            .request(Method::POST, url)
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await?;

        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow!(
                "DeepSeek 调用失败：认证失败，请检查 DEEPSEEK_API_KEY"
            ));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "DeepSeek 调用失败：状态码 {}，返回内容：{}",
                status,
                body
            ));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .context("DeepSeek Chat 返回解析失败")?;

        let message = chat_response
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .ok_or_else(|| anyhow!("DeepSeek Chat 返回结果为空"))?;

        Ok(message)
    }
}
