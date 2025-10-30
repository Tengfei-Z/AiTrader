use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FunctionCallRequest {
    pub function: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FunctionCallResponse {
    pub output: serde_json::Value,
    #[serde(default)]
    pub usage: Option<serde_json::Value>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiRequest<'a> {
    pub model: &'a str,
    #[serde(rename = "function_call")]
    pub function_call: &'a FunctionCallRequest,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: Vec<ChatMessage<'a>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage<'a> {
    pub role: &'a str,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatChoice {
    pub message: ChatReplyMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatReplyMessage {
    pub role: String,
    pub content: String,
}
