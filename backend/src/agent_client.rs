use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct AgentAnalysisRequest {
    pub session_id: String,
    pub instrument_id: String,
    pub analysis_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentAnalysisResponse {
    pub session_id: String,
    pub instrument_id: String,
    pub analysis_type: String,
    pub summary: String,
    #[serde(default)]
    pub suggestions: Vec<String>,
    pub created_at: String,
}

#[derive(Clone)]
pub struct AgentClient {
    http: Client,
    base_url: String,
}

impl AgentClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to build agent HTTP client")?;

        Ok(Self {
            http,
            base_url: base_url.into(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    pub async fn analysis(&self, request: AgentAnalysisRequest) -> Result<AgentAnalysisResponse> {
        let url = self.url("/analysis/");
        let response = self
            .http
            .post(url)
            .json(&request)
            .send()
            .await
            .context("failed to call agent analysis endpoint")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            return Err(anyhow!(
                "Agent analysis failed with status {}: {}",
                status,
                body
            ));
        }

        response
            .json::<AgentAnalysisResponse>()
            .await
            .context("failed to parse agent analysis response")
    }
}
