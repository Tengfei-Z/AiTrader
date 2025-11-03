use anyhow::{anyhow, ensure, Context, Result};
use okx::{
    models::{ClosePositionRequest, PlaceOrderRequest, SetTradingStopRequest, Ticker as OkxTicker},
    OkxRestClient,
};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TradeAction {
    OpenLong,
    OpenShort,
    ClosePosition,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct TradeExitPlanInput {
    pub profit_target: Option<f64>,
    pub stop_loss: Option<f64>,
    pub invalidation_condition: Option<String>,
}

impl Default for TradeExitPlanInput {
    fn default() -> Self {
        Self {
            profit_target: None,
            stop_loss: None,
            invalidation_condition: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct ExecuteTradeRequest {
    pub action: TradeAction,
    pub coin: String,
    pub instrument_id: Option<String>,
    pub instrument_type: Option<String>,
    pub quote: String,
    pub td_mode: String,
    pub margin_currency: Option<String>,
    pub leverage: Option<f64>,
    pub margin_amount: Option<f64>,
    pub quantity: Option<f64>,
    pub position_id: Option<String>,
    pub exit_plan: Option<TradeExitPlanInput>,
    pub confidence: Option<u8>,
    pub simulated_trading: bool,
}

impl Default for ExecuteTradeRequest {
    fn default() -> Self {
        Self {
            action: TradeAction::OpenLong,
            coin: String::new(),
            instrument_id: None,
            instrument_type: None,
            quote: default_quote(),
            td_mode: default_td_mode(),
            margin_currency: None,
            leverage: None,
            margin_amount: None,
            quantity: None,
            position_id: None,
            exit_plan: None,
            confidence: None,
            simulated_trading: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(default)]
pub struct ExecuteTradeResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrument_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notional_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidation_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    pub message: String,
}

impl Default for ExecuteTradeResponse {
    fn default() -> Self {
        Self {
            success: false,
            position_id: None,
            instrument_id: None,
            pos_side: None,
            entry_price: None,
            quantity: None,
            notional_value: None,
            liquidation_price: None,
            order_id: None,
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct UpdateExitPlanRequest {
    pub position_id: String,
    pub new_profit_target: Option<f64>,
    pub new_stop_loss: Option<f64>,
    pub new_invalidation: Option<String>,
    pub instrument_id: Option<String>,
    pub td_mode: Option<String>,
    pub simulated_trading: bool,
}

impl Default for UpdateExitPlanRequest {
    fn default() -> Self {
        Self {
            position_id: String::new(),
            new_profit_target: None,
            new_stop_loss: None,
            new_invalidation: None,
            instrument_id: None,
            td_mode: None,
            simulated_trading: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(default)]
pub struct UpdateExitPlanResponse {
    pub success: bool,
    pub position_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrument_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_take_profit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_stop_loss: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalidation: Option<String>,
    pub message: String,
}

impl Default for UpdateExitPlanResponse {
    fn default() -> Self {
        Self {
            success: false,
            position_id: String::new(),
            instrument_id: None,
            pos_side: None,
            applied_take_profit: None,
            applied_stop_loss: None,
            invalidation: None,
            message: String::new(),
        }
    }
}

pub async fn execute_trade(
    client: &OkxRestClient,
    request: &ExecuteTradeRequest,
) -> Result<ExecuteTradeResponse> {
    let inst_id = resolve_instrument_id(request);
    let inst_type = resolve_instrument_type(&inst_id, request.instrument_type.as_deref());

    match request.action {
        TradeAction::OpenLong | TradeAction::OpenShort => {
            open_position(client, request, &inst_id, &inst_type).await
        }
        TradeAction::ClosePosition => close_position(client, request, &inst_id, &inst_type).await,
    }
}

pub async fn update_exit_plan(
    client: &OkxRestClient,
    request: &UpdateExitPlanRequest,
) -> Result<UpdateExitPlanResponse> {
    let (inst_id, pos_side) = parse_position_id(request)
        .ok_or_else(|| anyhow!("position_id 格式应为 instId[:posSide]"))?;

    let td_mode = normalize_td_mode(request.td_mode.as_deref().unwrap_or(""));

    ensure!(
        request.new_profit_target.is_some() || request.new_stop_loss.is_some(),
        "至少需要设置 new_profit_target 或 new_stop_loss"
    );

    let trading_stop_request = SetTradingStopRequest {
        inst_id: inst_id.clone(),
        td_mode: td_mode.clone(),
        pos_side: pos_side.clone(),
        ccy: None,
        tp_trigger_px: request.new_profit_target.map(|value| format_decimal(value)),
        tp_ord_px: request.new_profit_target.map(|_| "-1".to_string()),
        sl_trigger_px: request.new_stop_loss.map(|value| format_decimal(value)),
        sl_ord_px: request.new_stop_loss.map(|_| "-1".to_string()),
        trigger_px_type: Some("last".to_string()),
        tp_trigger_px_type: Some("last".to_string()),
        sl_trigger_px_type: Some("last".to_string()),
    };

    client
        .set_trading_stop(&trading_stop_request)
        .await
        .context("更新止盈止损失败")?;

    let position_id = format_position_id(&inst_id, pos_side.as_deref());
    Ok(UpdateExitPlanResponse {
        success: true,
        position_id: position_id.clone(),
        instrument_id: Some(inst_id),
        pos_side,
        applied_take_profit: request.new_profit_target,
        applied_stop_loss: request.new_stop_loss,
        invalidation: request.new_invalidation.clone(),
        message: "退出计划已更新".to_string(),
    })
}

async fn open_position(
    client: &OkxRestClient,
    request: &ExecuteTradeRequest,
    inst_id: &str,
    inst_type: &str,
) -> Result<ExecuteTradeResponse> {
    let pos_side = match request.action {
        TradeAction::OpenLong => Some("long".to_string()),
        TradeAction::OpenShort => Some("short".to_string()),
        TradeAction::ClosePosition => request
            .position_id
            .as_ref()
            .and_then(|id| id.split(':').nth(1))
            .map(|value| value.to_string()),
    };

    let leverage = request.leverage.unwrap_or(1.0);
    ensure!(
        leverage.is_finite() && leverage > 0.0,
        "leverage 必须为正数"
    );

    let ticker = client
        .get_ticker(inst_id)
        .await
        .with_context(|| format!("获取 {inst_id} 最新价格失败"))?;
    let market_price = parse_price(&ticker)?;

    let (quantity, notional_value) = determine_trade_size(request, market_price)?;
    ensure!(quantity > 0.0, "无法确定下单数量");

    let td_mode = normalize_td_mode(&request.td_mode);

    let order_request = PlaceOrderRequest {
        inst_id: inst_id.to_string(),
        td_mode: td_mode.clone(),
        ccy: request.margin_currency.clone(),
        cl_ord_id: None,
        tag: Some("ai-trader".to_string()),
        side: match request.action {
            TradeAction::OpenLong => "buy".to_string(),
            TradeAction::OpenShort => "sell".to_string(),
            TradeAction::ClosePosition => {
                return Err(anyhow!("close_position 行为不应调用 open_position"))
            }
        },
        pos_side: pos_side.clone(),
        ord_type: "market".to_string(),
        sz: Some(format_decimal(quantity)),
        notional: None,
        px: None,
        reduce_only: Some(false),
        tgt_ccy: None,
        lever: Some(format_decimal(leverage)),
    };

    let order_result = client
        .place_order(&order_request)
        .await
        .context("OKX 下单失败")?;
    info!(
        inst_id,
        order_id = %order_result.ord_id,
        "OKX 下单成功"
    );

    let position_detail = fetch_position_snapshot(client, inst_id, inst_type, pos_side.as_deref())
        .await
        .context("拉取最新持仓失败")?;

    let entry_price = parse_optional_number(position_detail.avg_px.as_deref());
    let filled_qty = parse_optional_number(position_detail.pos.as_deref());
    let liquidation_price = parse_optional_number(position_detail.liq_px.as_deref());

    let final_quantity = filled_qty.or(Some(quantity));
    let final_entry = entry_price.or(Some(market_price));
    let final_notional = match (final_quantity, final_entry) {
        (Some(q), Some(price)) => Some(q * price),
        _ => Some(notional_value),
    };

    if let Some(exit_plan) = request.exit_plan.as_ref() {
        if exit_plan.profit_target.is_some() || exit_plan.stop_loss.is_some() {
            let trading_stop_request = SetTradingStopRequest {
                inst_id: inst_id.to_string(),
                td_mode: td_mode.clone(),
                pos_side: pos_side.clone(),
                ccy: request.margin_currency.clone(),
                tp_trigger_px: exit_plan.profit_target.map(|value| format_decimal(value)),
                tp_ord_px: exit_plan.profit_target.map(|_| "-1".to_string()),
                sl_trigger_px: exit_plan.stop_loss.map(|value| format_decimal(value)),
                sl_ord_px: exit_plan.stop_loss.map(|_| "-1".to_string()),
                trigger_px_type: Some("last".to_string()),
                tp_trigger_px_type: Some("last".to_string()),
                sl_trigger_px_type: Some("last".to_string()),
            };

            if let Err(err) = client.set_trading_stop(&trading_stop_request).await {
                warn!(
                    inst_id,
                    %err,
                    "设置退出计划失败，将继续返回下单成功的响应"
                );
            }
        }
    }

    let position_id = format_position_id(inst_id, pos_side.as_deref());

    Ok(ExecuteTradeResponse {
        success: true,
        position_id: Some(position_id),
        instrument_id: Some(inst_id.to_string()),
        pos_side,
        entry_price: final_entry,
        quantity: final_quantity,
        notional_value: final_notional,
        liquidation_price,
        order_id: Some(order_result.ord_id),
        message: "仓位开仓成功".to_string(),
    })
}

async fn close_position(
    client: &OkxRestClient,
    request: &ExecuteTradeRequest,
    inst_id: &str,
    inst_type: &str,
) -> Result<ExecuteTradeResponse> {
    let td_mode = normalize_td_mode(&request.td_mode);

    let existing_position = fetch_position_snapshot(
        client,
        inst_id,
        inst_type,
        request
            .position_id
            .as_ref()
            .and_then(|id| id.split(':').nth(1)),
    )
    .await
    .with_context(|| format!("未找到可以关闭的 {inst_id} 仓位"))?;

    let pos_side = existing_position.pos_side.clone();
    let entry_price = parse_optional_number(existing_position.avg_px.as_deref());
    let quantity = parse_optional_number(existing_position.pos.as_deref());
    let liquidation_price = parse_optional_number(existing_position.liq_px.as_deref());

    let close_request = ClosePositionRequest {
        inst_id: inst_id.to_string(),
        mgn_mode: td_mode,
        pos_side: pos_side.clone(),
        ccy: request.margin_currency.clone(),
    };

    client
        .close_position(&close_request)
        .await
        .context("OKX 平仓失败")?;

    let position_id = format_position_id(inst_id, pos_side.as_deref());

    Ok(ExecuteTradeResponse {
        success: true,
        position_id: Some(position_id),
        instrument_id: Some(inst_id.to_string()),
        pos_side,
        entry_price,
        quantity,
        notional_value: match (entry_price, quantity) {
            (Some(px), Some(sz)) => Some(px * sz),
            _ => None,
        },
        liquidation_price,
        order_id: None,
        message: "仓位已平仓".to_string(),
    })
}

async fn fetch_position_snapshot(
    client: &OkxRestClient,
    inst_id: &str,
    inst_type: &str,
    desired_pos_side: Option<&str>,
) -> Result<okx::models::PositionDetail> {
    let mut positions = client
        .get_positions(Some(inst_type))
        .await
        .context("查询持仓失败")?;

    positions.retain(|pos| pos.inst_id.eq_ignore_ascii_case(inst_id));

    if positions.is_empty() {
        return Err(anyhow!("未找到 {inst_id} 对应持仓"));
    }

    if let Some(pos_side) = desired_pos_side {
        if let Some(detail) = positions.iter().find(|pos| {
            pos.pos_side
                .as_deref()
                .map(|side| side.eq_ignore_ascii_case(pos_side))
                .unwrap_or(false)
        }) {
            return Ok(detail.clone());
        }
    }

    positions
        .into_iter()
        .find(|pos| {
            parse_optional_number(pos.pos.as_deref())
                .map(|qty| qty.abs() > f64::EPSILON)
                .unwrap_or(false)
        })
        .ok_or_else(|| anyhow!("未找到有效持仓"))
}

fn parse_price(ticker: &OkxTicker) -> Result<f64> {
    ticker
        .last
        .parse::<f64>()
        .context("无法解析最新成交价")
        .and_then(|price| {
            ensure!(price.is_finite() && price > 0.0, "最新成交价无效");
            Ok(price)
        })
}

fn parse_optional_number(value: Option<&str>) -> Option<f64> {
    value.and_then(|raw| raw.parse::<f64>().ok())
}

fn determine_trade_size(request: &ExecuteTradeRequest, market_price: f64) -> Result<(f64, f64)> {
    if let Some(quantity) = request.quantity {
        ensure!(
            quantity.is_finite() && quantity > 0.0,
            "quantity 必须为正数"
        );
        let notional = quantity * market_price;
        return Ok((quantity, notional));
    }

    let margin = request
        .margin_amount
        .ok_or_else(|| anyhow!("缺少 margin_amount 或 quantity"))?;
    ensure!(
        margin.is_finite() && margin > 0.0,
        "margin_amount 必须为正数"
    );

    let leverage = request.leverage.unwrap_or(1.0);
    let notional = margin * leverage;
    ensure!(
        market_price.is_finite() && market_price > 0.0,
        "行情价格无效"
    );
    let quantity = notional / market_price;
    ensure!(
        quantity.is_finite() && quantity > 0.0,
        "无法根据保证金计算下单数量"
    );

    Ok((quantity, notional))
}

fn resolve_instrument_id(request: &ExecuteTradeRequest) -> String {
    if let Some(ref inst) = request.instrument_id {
        return inst.to_uppercase();
    }

    let raw = request.coin.trim();
    if raw.contains('-') {
        return raw.to_uppercase();
    }

    format!(
        "{}-{}-SWAP",
        raw.to_uppercase(),
        request.quote.trim().to_uppercase()
    )
}

fn resolve_instrument_type(inst_id: &str, override_type: Option<&str>) -> String {
    if let Some(value) = override_type {
        return value.to_uppercase();
    }

    if inst_id.ends_with("-SWAP") {
        "SWAP".to_string()
    } else if inst_id.contains('-') {
        "SPOT".to_string()
    } else {
        "SWAP".to_string()
    }
}

fn default_td_mode() -> String {
    "cross".to_string()
}

fn default_quote() -> String {
    "USDT".to_string()
}

fn normalize_td_mode(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default_td_mode()
    } else {
        trimmed.to_lowercase()
    }
}

fn format_decimal(value: f64) -> String {
    let mut formatted = format!("{value:.8}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    if formatted.is_empty() {
        formatted.push('0');
    }
    formatted
}

fn parse_position_id(request: &UpdateExitPlanRequest) -> Option<(String, Option<String>)> {
    if let Some(ref inst) = request.instrument_id {
        return Some((inst.to_uppercase(), extract_pos_side(&request.position_id)));
    }

    let mut parts = request.position_id.splitn(2, ':');
    let inst_id = parts.next()?.trim();
    if inst_id.is_empty() {
        return None;
    }
    let pos_side = parts.next().and_then(|side| {
        let trimmed = side.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    Some((inst_id.to_uppercase(), pos_side))
}

fn format_position_id(inst_id: &str, pos_side: Option<&str>) -> String {
    match pos_side {
        Some(side) if !side.is_empty() => format!("{}:{}", inst_id, side),
        _ => inst_id.to_string(),
    }
}

fn extract_pos_side(position_id: &str) -> Option<String> {
    position_id
        .split(':')
        .nth(1)
        .map(|side| side.trim().to_string())
        .filter(|side| !side.is_empty())
}
