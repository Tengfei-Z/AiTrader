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
