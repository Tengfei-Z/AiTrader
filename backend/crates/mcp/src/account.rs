use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use okx::{models::PositionDetail, OkxRestClient};
use serde::{Deserialize, Serialize};

use rmcp::schemars;

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct AccountStateRequest {
    pub include_positions: bool,
    pub include_history: bool,
    pub include_performance: bool,
}

impl Default for AccountStateRequest {
    fn default() -> Self {
        Self {
            include_positions: true,
            include_history: false,
            include_performance: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct AccountState {
    pub account_value: f64,
    pub available_cash: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pnl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_fees: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_realized: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharpe_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_count: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub active_positions: Vec<PositionState>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PositionState {
    pub coin: String,
    pub side: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leverage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidation_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_plan: Option<ExitPlan>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ExitPlan {
    pub profit_target: Option<f64>,
    pub stop_loss: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalidation: Option<String>,
}

pub async fn fetch_account_state(
    client: &OkxRestClient,
    request: &AccountStateRequest,
) -> Result<AccountState> {
    let balance = client
        .get_account_balance()
        .await
        .context("拉取 OKX 账户余额失败")?;

    let balance = balance
        .data
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("OKX 账户余额响应为空"))?;

    let account_value = parse_amount(balance.total_eq.as_deref()).unwrap_or_default();
    let available_cash = balance
        .details
        .iter()
        .filter_map(|detail| parse_amount(detail.avail_bal.as_deref()))
        .sum();

    let mut total_unrealized = None;
    let mut positions_summary = Vec::new();

    if request.include_positions {
        let positions = client
            .get_positions(None)
            .await
            .context("拉取 OKX 持仓失败")?;

        let mut instrument_cache: HashMap<String, f64> = HashMap::new();
        for detail in positions.into_iter().filter(|pos| has_open_quantity(pos)) {
            if let Some(summary) = build_position_state(client, detail, &mut instrument_cache).await
            {
                if let Some(pnl) = summary.unrealized_pnl {
                    total_unrealized = Some(total_unrealized.unwrap_or_default() + pnl);
                }
                positions_summary.push(summary);
            }
        }
    }

    Ok(AccountState {
        account_value,
        available_cash,
        total_pnl: total_unrealized,
        total_fees: None,
        net_realized: None,
        sharpe_ratio: None,
        win_rate: None,
        trade_count: None,
        active_positions: positions_summary,
    })
}

fn parse_amount(value: Option<&str>) -> Option<f64> {
    value
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|amount| amount.is_finite())
}

fn has_open_quantity(detail: &PositionDetail) -> bool {
    detail
        .pos
        .as_deref()
        .and_then(|qty| qty.parse::<f64>().ok())
        .map(|qty| qty.abs() > f64::EPSILON)
        .unwrap_or(false)
}

async fn build_position_state(
    client: &OkxRestClient,
    detail: PositionDetail,
    price_cache: &mut HashMap<String, f64>,
) -> Option<PositionState> {
    let quantity = parse_amount(detail.pos.as_deref());
    let entry_price = parse_amount(detail.avg_px.as_deref());
    let leverage = parse_amount(detail.lever.as_deref());
    let liquidation_price = parse_amount(detail.liq_px.as_deref());
    let margin = parse_amount(detail.margin.as_deref());
    let unrealized_pnl = parse_amount(detail.upl.as_deref());

    let mut current_price =
        parse_amount(detail.mark_px.as_deref()).or_else(|| parse_amount(detail.last.as_deref()));

    if current_price.is_none() {
        if let Some(price) = price_cache.get(&detail.inst_id).copied() {
            current_price = Some(price);
        } else if let Ok(ticker) = client.get_ticker(&detail.inst_id).await {
            if let Some(price) = parse_amount(Some(&ticker.last)) {
                price_cache.insert(detail.inst_id.clone(), price);
                current_price = Some(price);
            }
        }
    }

    let entry_time = detail
        .c_time
        .as_deref()
        .and_then(|ts| parse_timestamp(ts).ok());

    Some(PositionState {
        coin: detail.inst_id.clone(),
        side: detail.pos_side.unwrap_or_else(|| "net".to_string()),
        entry_price,
        entry_time,
        quantity,
        leverage,
        liquidation_price,
        margin,
        unrealized_pnl,
        current_price,
        exit_plan: None,
    })
}

fn parse_timestamp(ts: &str) -> Result<String> {
    let millis: i64 = ts.parse().context("无法解析 OKX 时间戳")?;
    let seconds = millis.div_euclid(1000);
    let nanos_part = millis.rem_euclid(1000) as u32 * 1_000_000;
    let datetime: DateTime<Utc> =
        DateTime::from_timestamp(seconds, nanos_part).ok_or_else(|| anyhow!("时间戳超出范围"))?;
    Ok(datetime.to_rfc3339())
}
