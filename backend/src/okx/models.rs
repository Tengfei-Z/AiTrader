use super::error::OkxError;
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

    let signature = mac.finalize().into_bytes();

    Ok(BASE64_STANDARD.encode(signature))
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBalanceResponse {
    pub data: Vec<AccountBalance>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBalance {
    #[serde(default)]
    pub total_eq: Option<String>,
    #[serde(default)]
    pub details: Vec<AccountBalanceDetail>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBalanceDetail {
    pub ccy: String,
    #[serde(default)]
    pub cash_bal: Option<String>,
    #[serde(default)]
    pub avail_bal: Option<String>,
    #[serde(default)]
    pub eq: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionDetail {
    pub inst_id: String,
    #[serde(default)]
    pub inst_type: Option<String>,
    #[serde(default)]
    pub pos_side: Option<String>,
    #[serde(default)]
    pub avg_px: Option<String>,
    #[serde(default)]
    pub pos: Option<String>,
    #[serde(default)]
    pub lever: Option<String>,
    #[serde(default)]
    pub liq_px: Option<String>,
    #[serde(default)]
    pub margin: Option<String>,
    #[serde(default)]
    pub upl: Option<String>,
    #[serde(default)]
    pub mark_px: Option<String>,
    #[serde(default)]
    pub last: Option<String>,
    #[serde(default)]
    pub c_time: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FillDetail {
    #[serde(default)]
    pub inst_type: Option<String>,
    pub inst_id: String,
    #[serde(default)]
    pub trade_id: Option<String>,
    #[serde(default)]
    pub ord_id: Option<String>,
    #[serde(default)]
    pub cl_ord_id: Option<String>,
    #[serde(default)]
    pub fill_px: Option<String>,
    #[serde(default)]
    pub fill_sz: Option<String>,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(default)]
    pub pos_side: Option<String>,
    #[serde(default)]
    pub exec_type: Option<String>,
    #[serde(default)]
    pub fill_pnl: Option<String>,
    #[serde(default)]
    pub fee: Option<String>,
    #[serde(default)]
    pub ts: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionHistoryDetail {
    pub inst_id: String,
    #[serde(default)]
    pub pos_side: Option<String>,
    #[serde(default)]
    pub close_pos: Option<String>,
    #[serde(default)]
    pub open_avg_px: Option<String>,
    #[serde(default)]
    pub close_avg_px: Option<String>,
    #[serde(default)]
    pub lever: Option<String>,
    #[serde(default)]
    pub margin: Option<String>,
    #[serde(default)]
    pub pnl: Option<String>,
    #[serde(default)]
    pub pnl_ratio: Option<String>,
    #[serde(default)]
    pub c_time: Option<String>,
    #[serde(default)]
    pub u_time: Option<String>,
}
