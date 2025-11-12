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
    pub avail_eq: Option<String>,
    #[serde(default)]
    pub cash_bal: Option<String>,
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
    #[serde(default)]
    pub tp_trigger_px: Option<String>,
    #[serde(default)]
    pub tp_trigger_px_type: Option<String>,
    #[serde(default)]
    pub tp_ord_px: Option<String>,
    #[serde(default)]
    pub sl_trigger_px: Option<String>,
    #[serde(default)]
    pub sl_trigger_px_type: Option<String>,
    #[serde(default)]
    pub sl_ord_px: Option<String>,
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

fn deserialize_optional_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Option<bool>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "a bool or string that can be parsed as bool")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(v))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            serde::Deserialize::deserialize(deserializer).map(Some)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let normalized = value.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "true" | "1" | "yes" | "on" => Ok(Some(true)),
                "false" | "0" | "no" | "off" => Ok(Some(false)),
                _ => Err(E::custom("invalid boolean string")),
            }
        }
    }

    deserializer.deserialize_option(Visitor)
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderHistoryEntry {
    #[serde(default)]
    pub inst_type: Option<String>,
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "ordId")]
    pub ord_id: String,
    #[serde(rename = "clOrdId")]
    pub cl_ord_id: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    pub side: String,
    #[serde(rename = "posSide")]
    #[serde(default)]
    pub pos_side: Option<String>,
    #[serde(rename = "ordType")]
    pub ord_type: String,
    #[serde(rename = "tdMode")]
    #[serde(default)]
    pub td_mode: Option<String>,
    #[serde(rename = "sz")]
    pub sz: String,
    #[serde(rename = "accFillSz")]
    #[serde(default)]
    pub acc_fill_sz: String,
    #[serde(rename = "fillSz")]
    #[serde(default)]
    pub fill_sz: Option<String>,
    #[serde(rename = "fillPx")]
    #[serde(default)]
    pub fill_px: Option<String>,
    #[serde(rename = "px")]
    #[serde(default)]
    pub px: Option<String>,
    pub state: String,
    #[serde(rename = "lever")]
    #[serde(default)]
    pub lever: Option<String>,
    #[serde(rename = "reduceOnly")]
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_optional_bool")]
    pub reduce_only: Option<bool>,
    #[serde(rename = "tradeId")]
    #[serde(default)]
    pub trade_id: Option<String>,
    #[serde(rename = "uTime")]
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(rename = "cTime")]
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OrderHistoryResponse {
    pub data: Vec<OrderHistoryEntry>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FillResponse {
    pub data: Vec<FillDetail>,
}
