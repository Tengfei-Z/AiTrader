use crate::error::OkxError;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    pub inst_id: String,
    pub last: String,
    pub bid_px: Option<String>,
    pub ask_px: Option<String>,
    pub high_24h: Option<String>,
    pub low_24h: Option<String>,
    pub vol_24h: Option<String>,
    pub ts: String,
}

/// Create OKX-compliant signature for signed requests.
pub fn sign_request(
    timestamp: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
    secret_key: &str,
) -> Result<String, OkxError> {
    let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
        .map_err(|err| OkxError::Signature(err.to_string()))?;
    mac.update(timestamp.as_bytes());
    mac.update(method.as_bytes());
    mac.update(path.as_bytes());
    if let Some(payload) = body {
        mac.update(payload.as_bytes());
    }

    let signature = mac
        .finalize()
        .into_bytes();

    Ok(BASE64_STANDARD.encode(signature))
}
