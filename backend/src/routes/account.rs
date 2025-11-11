use std::time::Duration;

use axum::{
    extract::{Json, Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use crate::db;
use crate::db::{
    fetch_balance_snapshots, fetch_initial_equity, fetch_latest_balance_snapshot,
    fetch_order_history, insert_balance_snapshot, insert_initial_equity, BalanceSnapshotInsert,
};
use crate::okx::{self, OkxRestClient};
use crate::settings::CONFIG;
use crate::types::ApiResponse;
use crate::AppState;
use anyhow::Result;

const DEFAULT_BALANCE_SNAPSHOT_LIMIT: usize = 100;
const MAX_BALANCE_SNAPSHOT_LIMIT: usize = 1000;
const DEFAULT_INITIAL_EQUITY: f64 = 122_000.0;
const BALANCE_ASSET: &str = "USDT";
const BALANCE_SOURCE: &str = "okx";
const BALANCE_SNAPSHOT_TOLERANCE: f64 = 1e-6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub asset: String,
    pub available: String,
    pub locked: String,
    pub valuation_usdt: String,
}

#[derive(Debug)]
struct AccountBalancePayload {
    balance: Balance,
    available: f64,
    locked: f64,
    valuation: f64,
}

fn format_amount(value: f64) -> String {
    format!("{value:.6}")
}

fn build_balance_payload(account: okx::models::AccountBalance) -> Option<AccountBalancePayload> {
    let usdt_detail = account
        .details
        .into_iter()
        .find(|detail| detail.ccy.eq_ignore_ascii_case(BALANCE_ASSET));

    let detail_equity = usdt_detail
        .as_ref()
        .and_then(|detail| parse_optional_number(detail.eq.clone()));
    let detail_available = usdt_detail
        .as_ref()
        .and_then(|detail| parse_optional_number(detail.avail_bal.clone()));

    let account_value = detail_equity
        .or_else(|| parse_optional_number(account.total_eq.clone()))
        .unwrap_or(0.0);
    let available_cash = detail_available
        .or_else(|| parse_optional_number(account.avail_eq.clone()))
        .or_else(|| parse_optional_number(account.cash_bal.clone()))
        .unwrap_or(0.0);
    let locked = (account_value - available_cash).max(0.0);

    Some(AccountBalancePayload {
        balance: Balance {
            asset: BALANCE_ASSET.to_string(),
            available: format_amount(available_cash),
            locked: format_amount(locked),
            valuation_usdt: format_amount(account_value),
        },
        available: available_cash,
        locked,
        valuation: account_value,
    })
}

async fn fetch_account_balance_payload(
    client: &OkxRestClient,
) -> Result<Option<AccountBalancePayload>> {
    let response = client.get_account_balance().await?;
    Ok(response
        .data
        .into_iter()
        .next()
        .and_then(build_balance_payload))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSnapshotResponse {
    pub asset: String,
    pub available: String,
    pub locked: String,
    pub valuation: String,
    pub source: String,
    pub recorded_at: String,
}

#[derive(Debug, Deserialize)]
struct BalanceSnapshotListQuery {
    limit: Option<usize>,
    asset: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BalanceLatestQuery {
    asset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitialEquityRecord {
    amount: String,
    recorded_at: String,
}

#[derive(Debug, Deserialize)]
struct InitialEquityPayload {
    amount: f64,
}

fn default_initial_equity_record() -> InitialEquityRecord {
    InitialEquityRecord {
        amount: format_amount(DEFAULT_INITIAL_EQUITY),
        recorded_at: Utc::now().to_rfc3339(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Position {
    symbol: String,
    side: String,
    entry_price: Option<f64>,
    current_price: Option<f64>,
    quantity: Option<f64>,
    leverage: Option<f64>,
    liquidation_price: Option<f64>,
    margin: Option<f64>,
    unrealized_pnl: Option<f64>,
    entry_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_trigger: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_trigger: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PositionHistory {
    symbol: String,
    side: String,
    quantity: Option<f64>,
    leverage: Option<f64>,
    entry_price: Option<f64>,
    exit_price: Option<f64>,
    margin: Option<f64>,
    realized_pnl: Option<f64>,
    entry_time: Option<String>,
    exit_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Order {
    order_id: String,
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    order_type: String,
    price: Option<String>,
    size: String,
    filled_size: String,
    status: OrderStatus,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Fill {
    fill_id: String,
    order_id: String,
    symbol: String,
    side: String,
    price: String,
    size: String,
    fee: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pnl: Option<String>,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
struct SymbolOptionalQuery {
    symbol: Option<String>,
    limit: Option<usize>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/balances", get(get_balances))
        .route("/balances/snapshots", get(get_balance_snapshots))
        .route("/balances/latest", get(get_balance_latest))
        .route(
            "/initial-equity",
            get(get_initial_equity).post(set_initial_equity),
        )
        .route("/positions", get(get_positions))
        .route("/positions/history", get(get_positions_history))
        .route("/orders/open", get(get_open_orders))
        .route("/fills", get(get_fills))
}

async fn get_balances(State(state): State<AppState>) -> impl IntoResponse {
    let use_simulated = CONFIG.okx_use_simulated();
    tracing::info!(use_simulated, "received balances request");

    if let Some(client) = state.okx_simulated.clone() {
        match fetch_account_balance_payload(&client).await {
            Ok(Some(payload)) => {
                tracing::info!(available = payload.available, "okx balances refreshed");
                return Json(ApiResponse::ok(payload.balance));
            }
            Ok(None) => tracing::trace!("okx balances empty payload"),
            Err(err) => tracing::warn!(error = ?err, "failed to refresh balances"),
        }
    }

    Json(ApiResponse::<Balance>::error("failed to fetch balances"))
}

async fn get_balance_snapshots(
    Query(params): Query<BalanceSnapshotListQuery>,
) -> impl IntoResponse {
    let limit = params
        .limit
        .unwrap_or(DEFAULT_BALANCE_SNAPSHOT_LIMIT)
        .min(MAX_BALANCE_SNAPSHOT_LIMIT);
    let asset = params.asset.unwrap_or_else(|| BALANCE_ASSET.to_string());

    tracing::info!(asset = %asset, limit, "received balance snapshots request");

    match fetch_balance_snapshots(&asset, limit as i64).await {
        Ok(records) => {
            let payload: Vec<BalanceSnapshotResponse> = records
                .into_iter()
                .map(|record| BalanceSnapshotResponse {
                    asset: record.asset,
                    available: format_amount(record.available),
                    locked: format_amount(record.locked),
                    valuation: format_amount(record.valuation),
                    source: record.source,
                    recorded_at: record.recorded_at.to_rfc3339(),
                })
                .collect();
            Json(ApiResponse::ok(payload))
        }
        Err(err) => {
            tracing::warn!(error = ?err, "failed to fetch balance snapshots");
            Json(ApiResponse::<Vec<BalanceSnapshotResponse>>::error(
                "unable to fetch balance snapshots",
            ))
        }
    }
}

async fn get_balance_latest(
    Query(params): Query<BalanceLatestQuery>,
) -> Json<ApiResponse<Option<BalanceSnapshotResponse>>> {
    let asset = params.asset.unwrap_or_else(|| BALANCE_ASSET.to_string());
    tracing::info!(asset = %asset, "received balance latest request");

    match fetch_latest_balance_snapshot(&asset).await {
        Ok(Some(record)) => Json(ApiResponse::ok(Some(BalanceSnapshotResponse {
            asset: record.asset,
            available: format_amount(record.available),
            locked: format_amount(record.locked),
            valuation: format_amount(record.valuation),
            source: record.source,
            recorded_at: record.recorded_at.to_rfc3339(),
        }))),
        Ok(None) => Json(ApiResponse::ok(None)),
        Err(err) => {
            tracing::warn!(error = ?err, "failed to fetch latest balance snapshot");
            Json(ApiResponse::<Option<BalanceSnapshotResponse>>::error(
                "unable to read latest balance snapshot",
            ))
        }
    }
}

async fn get_initial_equity() -> Json<ApiResponse<Option<InitialEquityRecord>>> {
    let initial = match fetch_initial_equity().await {
        Ok(Some((amount, recorded_at))) => InitialEquityRecord {
            amount: format_amount(amount),
            recorded_at: recorded_at.to_rfc3339(),
        },
        Ok(None) => default_initial_equity_record(),
        Err(_) => default_initial_equity_record(),
    };
    Json(ApiResponse::ok(Some(initial)))
}

async fn set_initial_equity(
    Json(payload): Json<InitialEquityPayload>,
) -> Json<ApiResponse<Option<InitialEquityRecord>>> {
    if payload.amount < 0.0 {
        return Json(ApiResponse::<Option<InitialEquityRecord>>::error(
            "初始资金不能为负值",
        ));
    }

    if let Err(err) = insert_initial_equity(payload.amount).await {
        tracing::warn!(error = ?err, "failed to write initial equity");
        return Json(ApiResponse::ok(Some(InitialEquityRecord {
            amount: format_amount(payload.amount),
            recorded_at: Utc::now().to_rfc3339(),
        })));
    }

    get_initial_equity().await
}

async fn get_positions(State(state): State<AppState>) -> impl IntoResponse {
    let use_simulated = CONFIG.okx_use_simulated();
    tracing::info!(use_simulated, "received positions request");

    if let Some(client) = state.okx_simulated.clone() {
        if let Ok(position_details) = client.get_positions(None).await {
            let mut positions = Vec::new();

            for detail in position_details {
                if !detail.inst_id.to_ascii_uppercase().contains("USDT") {
                    continue;
                }

                let quantity = parse_optional_number(detail.pos.clone());
                if quantity
                    .map(|qty| qty.abs() > f64::EPSILON)
                    .unwrap_or(false)
                {
                    let current_price = parse_optional_number(detail.mark_px.clone())
                        .or_else(|| parse_optional_number(detail.last.clone()));

                    positions.push(Position {
                        symbol: detail.inst_id.clone(),
                        side: detail
                            .pos_side
                            .unwrap_or_else(|| "net".to_string())
                            .to_lowercase(),
                        entry_price: parse_optional_number(detail.avg_px.clone()),
                        current_price,
                        quantity,
                        leverage: parse_optional_number(detail.lever.clone()),
                        liquidation_price: parse_optional_number(detail.liq_px.clone()),
                        margin: parse_optional_number(detail.margin.clone()),
                        unrealized_pnl: parse_optional_number(detail.upl.clone()),
                        entry_time: parse_timestamp_millis(detail.c_time.clone()),
                        take_profit_trigger: parse_optional_number(detail.tp_trigger_px.clone()),
                        take_profit_price: parse_optional_number(detail.tp_ord_px.clone()),
                        take_profit_type: optional_string(detail.tp_trigger_px_type.clone()),
                        stop_loss_trigger: parse_optional_number(detail.sl_trigger_px.clone()),
                        stop_loss_price: parse_optional_number(detail.sl_ord_px.clone()),
                        stop_loss_type: optional_string(detail.sl_trigger_px_type.clone()),
                    });
                }
            }

            tracing::info!(
                use_simulated,
                position_count = positions.len(),
                "okx positions parsed"
            );
            return Json(ApiResponse::ok(positions));
        } else {
            tracing::warn!(use_simulated, "failed to fetch OKX positions");
        }
    }

    Json(ApiResponse::ok(Vec::<Position>::new()))
}

async fn get_open_orders(
    _state: State<AppState>,
    Query(SymbolOptionalQuery {
        symbol: _symbol, ..
    }): Query<SymbolOptionalQuery>,
) -> impl IntoResponse {
    tracing::info!("received open orders request");
    Json(ApiResponse::ok(Vec::<Order>::new()))
}

async fn get_fills(
    State(state): State<AppState>,
    Query(params): Query<SymbolOptionalQuery>,
) -> impl IntoResponse {
    let use_simulated = CONFIG.okx_use_simulated();
    let symbol_filter = params.symbol.clone();
    let limit = params.limit;

    tracing::info!(
        ?symbol_filter,
        use_simulated,
        limit,
        "received fills request"
    );

    if let Some(client) = state.okx_simulated.clone() {
        if let Ok(remote_fills) = client.get_fills(symbol_filter.as_deref(), limit).await {
            let mut fills: Vec<Fill> = remote_fills.into_iter().map(convert_okx_fill).collect();
            if let Some(limit) = limit {
                fills.truncate(limit);
            }
            return Json(ApiResponse::ok(fills));
        } else {
            tracing::warn!(use_simulated, "failed to fetch OKX fills");
        }
    }

    Json(ApiResponse::ok(Vec::<Fill>::new()))
}

async fn get_positions_history(
    State(_state): State<AppState>,
    Query(params): Query<SymbolOptionalQuery>,
) -> impl IntoResponse {
    let limit = params.limit.map(|v| v as i64);
    let symbol_filter = params.symbol.clone();

    tracing::info!(?symbol_filter, limit, "received positions history request");

    match fetch_order_history(limit).await {
        Ok(mut records) => {
            if let Some(symbol) = symbol_filter {
                records.retain(|record| record.symbol == symbol);
            }

            let history: Vec<PositionHistory> = records
                .into_iter()
                .map(convert_order_history_record)
                .collect();

            Json(ApiResponse::ok(history))
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to fetch order history from database");
            Json(ApiResponse::ok(Vec::<PositionHistory>::new()))
        }
    }
}

fn convert_okx_fill(detail: okx::models::FillDetail) -> Fill {
    let okx::models::FillDetail {
        inst_id,
        trade_id,
        ord_id,
        fill_px,
        fill_sz,
        side,
        fee,
        ts,
        fill_pnl,
        ..
    } = detail;

    let fill_id = trade_id
        .clone()
        .or_else(|| ord_id.clone())
        .or_else(|| ts.clone())
        .unwrap_or_else(|| inst_id.clone());

    let order_id = ord_id.unwrap_or_else(|| fill_id.clone());

    Fill {
        fill_id,
        order_id,
        symbol: inst_id,
        side: side
            .map(|value| value.to_lowercase())
            .unwrap_or_else(|| "unknown".to_string()),
        price: string_or_default(fill_px, "0"),
        size: string_or_default(fill_sz, "0"),
        fee: string_or_default(fee, "0"),
        pnl: optional_string(fill_pnl),
        timestamp: string_or_default(ts, "0"),
    }
}

fn optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn string_or_default(value: Option<String>, default: &str) -> String {
    optional_string(value).unwrap_or_else(|| default.to_string())
}

fn convert_order_history_record(record: db::OrderHistoryRecord) -> PositionHistory {
    let exit_price = record
        .metadata
        .get("exit_price")
        .and_then(|value| value.as_f64())
        .or_else(|| {
            record
                .metadata
                .get("avg_exit_price")
                .and_then(|value| value.as_f64())
        });
    let margin = record
        .metadata
        .get("margin_usdt")
        .and_then(|value| value.as_f64());
    let realized_pnl = record
        .metadata
        .get("realized_pnl_usdt")
        .and_then(|value| value.as_f64())
        .or_else(|| {
            record
                .metadata
                .get("pnl_usdt")
                .and_then(|value| value.as_f64())
        });

    PositionHistory {
        symbol: record.symbol,
        side: record.side.to_lowercase(),
        quantity: record.size,
        leverage: record.leverage,
        entry_price: record.price,
        exit_price,
        margin,
        realized_pnl,
        entry_time: Some(record.created_at.to_rfc3339()),
        exit_time: record.closed_at.map(|dt| dt.to_rfc3339()),
    }
}

fn parse_timestamp_millis(value: Option<String>) -> Option<String> {
    let millis = value?.parse::<i64>().ok()?;
    chrono::Utc
        .timestamp_millis_opt(millis)
        .single()
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

fn parse_optional_number(value: Option<String>) -> Option<f64> {
    optional_string(value)
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

async fn record_balance_snapshot_if_changed(
    available: f64,
    locked: f64,
    valuation: f64,
) -> anyhow::Result<()> {
    if let Some(previous) = fetch_latest_balance_snapshot(BALANCE_ASSET).await? {
        if (previous.valuation - valuation).abs() < BALANCE_SNAPSHOT_TOLERANCE {
            return Ok(());
        }
    }

    insert_balance_snapshot(BalanceSnapshotInsert {
        asset: BALANCE_ASSET.to_string(),
        available,
        locked,
        valuation,
        source: BALANCE_SOURCE.to_string(),
    })
    .await
}

pub async fn run_balance_snapshot_loop(state: AppState) {
    loop {
        if let Some(client) = state.okx_simulated.clone() {
            match fetch_account_balance_payload(&client).await {
                Ok(Some(payload)) => {
                    if let Err(err) = record_balance_snapshot_if_changed(
                        payload.available,
                        payload.locked,
                        payload.valuation,
                    )
                    .await
                    {
                        tracing::warn!(error = ?err, "periodic balance snapshot failed");
                    }
                }
                Ok(None) => tracing::trace!("periodic balance snapshot received empty payload"),
                Err(err) => {
                    tracing::warn!(error = ?err, "failed to refresh periodic balance snapshot")
                }
            }
        }
        sleep(Duration::from_secs(5)).await;
    }
}
