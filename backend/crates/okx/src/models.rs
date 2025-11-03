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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestResponse<T> {
    pub code: String,
    pub msg: String,
    pub data: Vec<T>,
}

impl<T> RestResponse<T> {
    pub fn ensure_success(mut self) -> Result<Vec<T>, OkxError> {
        if self.code != "0" {
            return Err(OkxError::Api {
                code: self.code,
                msg: self.msg,
            });
        }
        Ok(std::mem::take(&mut self.data))
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponseItem {
    pub ord_id: String,
    #[serde(default)]
    pub cl_ord_id: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    pub s_code: String,
    #[serde(default)]
    pub s_msg: Option<String>,
}

impl OrderResponseItem {
    pub fn ensure_success(&self) -> Result<(), OkxError> {
        if self.s_code != "0" {
            return Err(OkxError::ApiSub {
                code: self.s_code.clone(),
                msg: self.s_msg.clone().unwrap_or_default(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosePositionResponseItem {
    pub inst_id: String,
    #[serde(default)]
    pub pos_side: Option<String>,
    pub s_code: String,
    #[serde(default)]
    pub s_msg: Option<String>,
}

impl ClosePositionResponseItem {
    pub fn ensure_success(&self) -> Result<(), OkxError> {
        if self.s_code != "0" {
            return Err(OkxError::ApiSub {
                code: self.s_code.clone(),
                msg: self.s_msg.clone().unwrap_or_default(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetTradingStopResponseItem {
    pub inst_id: String,
    #[serde(default)]
    pub pos_side: Option<String>,
    pub s_code: String,
    #[serde(default)]
    pub s_msg: Option<String>,
}

impl SetTradingStopResponseItem {
    pub fn ensure_success(&self) -> Result<(), OkxError> {
        if self.s_code != "0" {
            return Err(OkxError::ApiSub {
                code: self.s_code.clone(),
                msg: self.s_msg.clone().unwrap_or_default(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOrderRequest {
    pub inst_id: String,
    pub td_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cl_ord_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub side: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_side: Option<String>,
    pub ord_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sz: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notional: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tgt_ccy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lever: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosePositionRequest {
    pub inst_id: String,
    pub mgn_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccy: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetTradingStopRequest {
    pub inst_id: String,
    pub td_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tp_trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tp_ord_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sl_trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sl_ord_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_px_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tp_trigger_px_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sl_trigger_px_type: Option<String>,
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
pub struct PositionsResponse {
    pub data: Vec<PositionDetail>,
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
