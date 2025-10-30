use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpRequest {
    pub tool: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpResponse {
    pub status: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}
