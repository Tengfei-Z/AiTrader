use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OkxError {
    #[error("http client error: {0}")]
    HttpClient(#[from] reqwest::Error),
    #[error("unexpected http status: {0}")]
    HttpStatus(StatusCode),
    #[error("failed to serialize or deserialize payload: {0}")]
    Deserialize(#[from] anyhow::Error),
    #[error("failed to serialize request payload: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("signature error: {0}")]
    Signature(String),
    #[error("empty response from {0}")]
    EmptyResponse(String),
    #[error("invalid header value: {0}")]
    Header(#[from] reqwest::header::InvalidHeaderValue),
}
