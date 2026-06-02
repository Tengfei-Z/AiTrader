use std::collections::HashMap;
use std::time::Duration;

use axum::{
    extract::{Json, Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::sleep;

use crate::db;
use crate::db::{
    fetch_balance_snapshots, fetch_initial_equity, fetch_latest_balance_snapshot,
    fetch_position_snapshots, insert_balance_snapshot, insert_initial_equity,
    BalanceSnapshotInsert, PositionSnapshot,
};
use crate::okx::{self, OkxRestClient};
use crate::settings::CONFIG;
use crate::types::ApiResponse;
use crate::AppState;
use anyhow::{anyhow, Result};

/// 默认返回的余额快照条数
const DEFAULT_BALANCE_SNAPSHOT_LIMIT: usize = 100;
/// 最大允许返回的余额快照条数
const MAX_BALANCE_SNAPSHOT_LIMIT: usize = 1000;
/// 余额资产类型（USDT 稳定币）
const BALANCE_ASSET: &str = "USDT";
/// 余额数据来源标识
const BALANCE_SOURCE: &str = "okx";
/// 余额快照变化容差阈值（小于此值的变化不记录新快照）
const BALANCE_SNAPSHOT_TOLERANCE: f64 = 1e-6;
/// 余额快照采样间隔（秒）
const BALANCE_SNAPSHOT_INTERVAL_SECS: u64 = 30 * 60;
/// 初始资金同步时的容差
const INITIAL_EQUITY_TOLERANCE: f64 = 1e-6;
/// DB fallback for current positions should not surface stale open rows forever.
const CURRENT_POSITION_STALE_AFTER_SECS: i64 = 5 * 60;

/// 账户余额信息（返回给前端的格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    /// 资产类型（如 USDT）
    pub asset: String,
    /// 可用余额（字符串格式，保留 6 位小数）
    pub available: String,
    /// 冻结余额（字符串格式，保留 6 位小数）
    pub locked: String,
    /// USDT 估值（字符串格式，保留 6 位小数）
    pub valuation_usdt: String,
}

/// 账户余额数据载荷（内部使用，包含数值格式）
#[derive(Debug)]
struct AccountBalancePayload {
    /// 格式化后的余额信息
    balance: Balance,
    /// 可用余额（数值）
    available: f64,
    /// 冻结余额（数值）
    locked: f64,
    /// 总估值（数值）
    valuation: f64,
}

/// 格式化金额为字符串（保留 6 位小数）
fn format_amount(value: f64) -> String {
    format!("{value:.6}")
}

/// 从 OKX 账户余额响应中构建余额载荷
///
/// 从账户详情中提取 USDT 资产信息，计算可用余额、冻结余额和总估值
fn build_balance_payload(account: okx::models::AccountBalance) -> Option<AccountBalancePayload> {
    // 找到 USDT 资产的详情
    let usdt_detail = account
        .details
        .into_iter()
        .find(|detail| detail.ccy.eq_ignore_ascii_case(BALANCE_ASSET));

    // 提取 USDT 资产的权益值
    let detail_equity = usdt_detail
        .as_ref()
        .and_then(|detail| parse_optional_number(detail.eq.clone()));
    // 提取 USDT 资产的可用余额
    let detail_available = usdt_detail
        .as_ref()
        .and_then(|detail| parse_optional_number(detail.avail_bal.clone()));

    // 账户总价值：优先使用 USDT 资产权益，否则使用账户总权益
    let account_value = detail_equity
        .or_else(|| parse_optional_number(account.total_eq.clone()))
        .unwrap_or(0.0);
    // 可用资金：优先使用 USDT 可用余额，否则使用账户可用权益或现金余额
    let available_cash = detail_available
        .or_else(|| parse_optional_number(account.avail_eq.clone()))
        .or_else(|| parse_optional_number(account.cash_bal.clone()))
        .unwrap_or(0.0);
    // 冻结资金 = 总价值 - 可用资金（确保非负）
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

/// 从 OKX 客户端获取账户余额载荷
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

/// 余额快照响应（返回给前端的格式）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSnapshotResponse {
    /// 资产类型
    pub asset: String,
    /// 可用余额
    pub available: String,
    /// 冻结余额
    pub locked: String,
    /// 总估值
    pub valuation: String,
    /// 数据来源（如 okx）
    pub source: String,
    /// 记录时间（ISO 8601 格式）
    pub recorded_at: String,
}

/// 余额快照列表查询参数
#[derive(Debug, Deserialize)]
struct BalanceSnapshotListQuery {
    /// 返回条数限制
    limit: Option<usize>,
    /// 资产类型过滤
    asset: Option<String>,
    /// 仅返回该时间点之后的记录（RFC3339）
    after: Option<String>,
}

/// 余额快照列表响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BalanceSnapshotListPayload {
    snapshots: Vec<BalanceSnapshotResponse>,
    has_more: bool,
    next_cursor: Option<String>,
}

/// 最新余额查询参数
#[derive(Debug, Deserialize)]
struct BalanceLatestQuery {
    /// 资产类型过滤
    asset: Option<String>,
}

/// 初始资金记录
#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitialEquityRecord {
    /// 初始资金金额
    amount: String,
    /// 记录时间
    recorded_at: String,
}

/// 设置初始资金的请求载荷
#[derive(Debug, Deserialize)]
struct InitialEquityPayload {
    /// 初始资金金额
    amount: f64,
}

/// 返回默认的初始资金记录（从配置读取）
fn default_initial_equity_record() -> InitialEquityRecord {
    InitialEquityRecord {
        amount: format_amount(CONFIG.initial_equity),
        recorded_at: Utc::now().to_rfc3339(),
    }
}

/// 当前/历史持仓快照（由 backend positions 表提供）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionSnapshotResponse {
    inst_id: String,
    pos_side: String,
    td_mode: Option<String>,
    side: String,
    size: f64,
    avg_price: Option<f64>,
    mark_px: Option<f64>,
    margin: Option<f64>,
    unrealized_pnl: Option<f64>,
    take_profit_px: Option<f64>,
    stop_loss_px: Option<f64>,
    last_trade_at: Option<String>,
    closed_at: Option<String>,
    action_kind: Option<String>,
    entry_ord_id: Option<String>,
    exit_ord_id: Option<String>,
    metadata: Value,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderResponse {
    order_id: String,
    symbol: String,
    side: String,
    pos_side: Option<String>,
    order_type: String,
    price: Option<String>,
    trigger_px: Option<String>,
    take_profit_px: Option<String>,
    stop_loss_px: Option<String>,
    size: String,
    filled_size: String,
    status: String,
    reduce_only: Option<bool>,
    close_on_trigger: Option<bool>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FillResponse {
    fill_id: String,
    order_id: String,
    symbol: String,
    side: String,
    price: String,
    size: String,
    fee: String,
    pnl: Option<String>,
    timestamp: String,
}

/// 交易对查询参数（可选）
#[derive(Debug, Deserialize)]
struct SymbolOptionalQuery {
    /// 交易对符号过滤
    symbol: Option<String>,
    /// 返回条数限制
    limit: Option<usize>,
}

/// 创建账户相关的路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/balances", get(get_balances)) // 获取当前余额
        .route("/balances/snapshots", get(get_balance_snapshots)) // 获取余额快照列表
        .route("/balances/latest", get(get_balance_latest)) // 获取最新余额快照
        .route(
            "/initial-equity",
            get(get_initial_equity).post(set_initial_equity), // 获取/设置初始资金
        )
        .route("/positions", get(get_positions)) // 获取当前持仓
        .route("/positions/history", get(get_positions_history)) // 获取历史持仓
        .route("/orders/open", get(get_open_orders)) // 获取当前未完成委托
        .route("/fills", get(get_fills)) // 获取历史成交
}

/// 获取当前账户余额
///
/// 从 OKX 交易所实时获取账户的 USDT 余额信息
async fn get_balances(State(state): State<AppState>) -> impl IntoResponse {
    let use_simulated = CONFIG.okx_use_simulated();
    tracing::info!(use_simulated, "received balances request");

    if let Some(client) = state.okx_client.clone() {
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

/// 获取余额快照列表
///
/// 从数据库中读取历史余额快照记录，用于显示账户余额变化曲线
async fn get_balance_snapshots(
    Query(params): Query<BalanceSnapshotListQuery>,
) -> impl IntoResponse {
    let requested_limit = params
        .limit
        .unwrap_or(DEFAULT_BALANCE_SNAPSHOT_LIMIT)
        .max(1)
        .min(MAX_BALANCE_SNAPSHOT_LIMIT);
    let fetch_limit = requested_limit.saturating_add(1);
    let asset = params.asset.unwrap_or_else(|| BALANCE_ASSET.to_string());
    let after = params.after.as_deref().and_then(|raw| {
        DateTime::parse_from_rfc3339(raw)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|err| tracing::warn!(%raw, error = ?err, "invalid after parameter"))
            .ok()
    });

    tracing::info!(
        asset = %asset,
        limit = requested_limit,
        after = %params.after.as_deref().unwrap_or("earliest"),
        "received balance snapshots request"
    );

    match fetch_balance_snapshots(&asset, fetch_limit as i64, after).await {
        Ok(mut records) => {
            let has_more = (records.len() as usize) == (fetch_limit as usize);
            if has_more {
                records.pop();
            }

            let snapshots: Vec<BalanceSnapshotResponse> = records
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

            let next_cursor = if has_more {
                snapshots
                    .last()
                    .map(|snapshot| snapshot.recorded_at.clone())
            } else {
                None
            };

            Json(ApiResponse::ok(BalanceSnapshotListPayload {
                snapshots,
                has_more,
                next_cursor,
            }))
        }
        Err(err) => {
            tracing::warn!(error = ?err, "failed to fetch balance snapshots");
            Json(ApiResponse::<BalanceSnapshotListPayload>::error(
                "unable to fetch balance snapshots",
            ))
        }
    }
}

/// 获取最新的余额快照
///
/// 从数据库中读取最近一次记录的余额快照
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

/// 获取初始资金记录
///
/// 从数据库读取用户设置的初始资金，用于计算收益率
async fn get_initial_equity() -> Json<ApiResponse<Option<InitialEquityRecord>>> {
    let env_override = CONFIG.initial_equity_env_override();
    let initial = if let Some(env_amount) = env_override {
        match ensure_initial_equity_from_env(env_amount).await {
            Ok(record) => record,
            Err(err) => {
                tracing::warn!(
                    error = ?err,
                    "failed to sync initial equity from environment, falling back to database"
                );
                load_initial_equity_from_db().await
            }
        }
    } else {
        load_initial_equity_from_db().await
    };
    Json(ApiResponse::ok(Some(initial)))
}

/// 设置初始资金
///
/// 允许用户设置或更新初始资金金额
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

/// 获取当前持仓列表
///
/// 优先从 OKX 实时返回当前持仓；DB 仅作为 OKX 不可用时的降级来源。
#[axum::debug_handler]
async fn get_positions(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<PositionSnapshotResponse>>> {
    if let Some(client) = state.okx_client.as_ref() {
        match client.get_positions(Some("SWAP")).await {
            Ok(details) => {
                let allowed_inst_ids = CONFIG.okx_inst_ids();
                let algo_orders = fetch_algo_pending_orders(client, None).await;
                let algo_orders_by_inst = group_algo_orders_by_inst(algo_orders);
                let mut snapshots = Vec::new();

                for detail in details
                    .into_iter()
                    .filter(|detail| allowed_inst_ids.contains(&detail.inst_id))
                {
                    let db_snapshot = position_snapshot_from_okx_detail(&detail);
                    if let Err(err) = db::upsert_position_snapshot(db_snapshot).await {
                        tracing::warn!(
                            error = ?err,
                            inst_id = %detail.inst_id,
                            "failed to refresh position snapshot from okx"
                        );
                    }

                    if okx_position_size(&detail).abs() > 1e-9 {
                        let related_algo_orders = algo_orders_by_inst
                            .get(&detail.inst_id)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]);
                        snapshots.push(convert_okx_position_detail(detail, related_algo_orders));
                    }
                }

                return Json(ApiResponse::ok(snapshots));
            }
            Err(err) => {
                tracing::warn!(error = ?err, "failed to fetch current positions from okx, falling back to database");
            }
        }
    }

    match fetch_position_snapshots(false, None, None).await {
        Ok(records) => {
            let snapshots: Vec<PositionSnapshotResponse> = records
                .into_iter()
                .filter(|record| {
                    record.size.abs() > 1e-9
                        && record.updated_at
                            >= Utc::now()
                                - ChronoDuration::seconds(CURRENT_POSITION_STALE_AFTER_SECS)
                })
                .map(convert_position_snapshot)
                .collect();
            Json(ApiResponse::ok(snapshots))
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to fetch current positions from database");
            Json(ApiResponse::ok(Vec::<PositionSnapshotResponse>::new()))
        }
    }
}

fn convert_okx_position_detail(
    detail: okx::models::PositionDetail,
    algo_orders: &[okx::models::AlgoOrderDetail],
) -> PositionSnapshotResponse {
    let size = okx_position_size(&detail);
    let pos_side = detail
        .pos_side
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "net".to_string());
    let side = infer_position_side(size, &pos_side, &Value::Null);

    PositionSnapshotResponse {
        inst_id: detail.inst_id.clone(),
        pos_side: pos_side.clone(),
        td_mode: detail.inst_type.clone(),
        side,
        size,
        avg_price: parse_optional_number(detail.avg_px.clone()),
        mark_px: parse_optional_number(detail.mark_px.clone()),
        margin: parse_optional_number(detail.margin.clone()),
        unrealized_pnl: parse_optional_number(detail.upl.clone()),
        take_profit_px: position_take_profit_px(&detail, algo_orders),
        stop_loss_px: position_stop_loss_px(&detail, algo_orders),
        last_trade_at: parse_okx_timestamp_ms(detail.c_time.as_deref()).map(|dt| dt.to_rfc3339()),
        closed_at: None,
        action_kind: None,
        entry_ord_id: None,
        exit_ord_id: None,
        metadata: serde_json::to_value(&detail).unwrap_or(Value::Null),
        updated_at: Utc::now().to_rfc3339(),
    }
}

fn position_snapshot_from_okx_detail(detail: &okx::models::PositionDetail) -> PositionSnapshot {
    let size = okx_position_size(detail);
    let pos_side = detail
        .pos_side
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "net".to_string());
    let side = infer_position_side(size, &pos_side, &Value::Null);

    PositionSnapshot {
        inst_id: detail.inst_id.clone(),
        pos_side: pos_side.clone(),
        td_mode: detail.inst_type.clone(),
        side,
        size,
        avg_price: parse_optional_number(detail.avg_px.clone()),
        mark_px: parse_optional_number(detail.mark_px.clone()),
        margin: parse_optional_number(detail.margin.clone()),
        unrealized_pnl: parse_optional_number(detail.upl.clone()),
        last_trade_at: parse_okx_timestamp_ms(detail.c_time.as_deref()),
        closed_at: if size.abs() <= 1e-9 {
            Some(Utc::now())
        } else {
            None
        },
        action_kind: None,
        entry_ord_id: None,
        exit_ord_id: None,
        metadata: serde_json::to_value(detail).unwrap_or(Value::Null),
    }
}

fn okx_position_size(detail: &okx::models::PositionDetail) -> f64 {
    parse_optional_number(detail.pos.clone()).unwrap_or(0.0)
}

fn parse_okx_timestamp_ms(value: Option<&str>) -> Option<DateTime<Utc>> {
    let text = value?.trim();
    let ms = text.parse::<i64>().ok()?;
    chrono::TimeZone::timestamp_millis_opt(&Utc, ms).single()
}

/// 获取历史持仓记录
///
/// 从本地 `positions` 表读取已平仓仓位，支持按交易对过滤
#[axum::debug_handler]
async fn get_positions_history(
    Query(params): Query<SymbolOptionalQuery>,
) -> Json<ApiResponse<Vec<PositionSnapshotResponse>>> {
    let limit = params.limit.map(|v| v as i64);
    let symbol_filter = params.symbol.as_deref();

    tracing::info!(?symbol_filter, limit, "received positions history request");

    match fetch_position_snapshots(true, symbol_filter, limit).await {
        Ok(records) => {
            let history: Vec<PositionSnapshotResponse> =
                records.into_iter().map(convert_position_snapshot).collect();
            Json(ApiResponse::ok(history))
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to fetch historical positions from database");
            Json(ApiResponse::ok(Vec::<PositionSnapshotResponse>::new()))
        }
    }
}

/// 获取当前未完成委托
#[axum::debug_handler]
async fn get_open_orders(
    State(state): State<AppState>,
    Query(params): Query<SymbolOptionalQuery>,
) -> Json<ApiResponse<Vec<OrderResponse>>> {
    let symbol_filter = params.symbol.as_deref();

    let Some(client) = state.okx_client.as_ref() else {
        return Json(ApiResponse::ok(Vec::new()));
    };

    let allowed_inst_ids = CONFIG.okx_inst_ids();
    let mut converted: Vec<OrderResponse> = match client
        .get_open_orders(Some("SWAP"), symbol_filter, None)
        .await
    {
        Ok(orders) => orders
            .into_iter()
            .filter(|order| allowed_inst_ids.contains(&order.inst_id))
            .map(convert_order)
            .collect(),
        Err(err) => {
            tracing::warn!(error = ?err, "failed to fetch open orders");
            Vec::new()
        }
    };

    converted.extend(
        fetch_algo_pending_orders(client, symbol_filter)
            .await
            .into_iter()
            .filter(|order| {
                order
                    .inst_id
                    .as_ref()
                    .is_some_and(|inst_id| allowed_inst_ids.contains(inst_id))
            })
            .map(convert_algo_order),
    );

    Json(ApiResponse::ok(converted))
}

/// 获取历史成交
#[axum::debug_handler]
async fn get_fills(
    State(state): State<AppState>,
    Query(params): Query<SymbolOptionalQuery>,
) -> Json<ApiResponse<Vec<FillResponse>>> {
    let symbol_filter = params.symbol.as_deref();
    let limit = params.limit.map(|value| value as i64);

    let Some(client) = state.okx_client.as_ref() else {
        return Json(ApiResponse::ok(Vec::new()));
    };

    match client.get_fills(symbol_filter, None, limit).await {
        Ok(fills) => {
            let allowed_inst_ids = CONFIG.okx_inst_ids();
            let converted = fills
                .into_iter()
                .filter(|fill| allowed_inst_ids.contains(&fill.inst_id))
                .map(convert_fill)
                .collect();
            Json(ApiResponse::ok(converted))
        }
        Err(err) => {
            tracing::warn!(error = ?err, "failed to fetch fills");
            Json(ApiResponse::ok(Vec::new()))
        }
    }
}

fn convert_order(order: okx::models::OrderHistoryEntry) -> OrderResponse {
    let trigger_px = optional_string(order.trigger_px.clone());
    let take_profit_px = optional_string(order.tp_trigger_px.clone())
        .or_else(|| trigger_px.clone().filter(|_| is_take_profit_order(&order)));
    let stop_loss_px = optional_string(order.sl_trigger_px.clone())
        .or_else(|| trigger_px.clone().filter(|_| is_stop_loss_order(&order)));

    OrderResponse {
        order_id: order.ord_id,
        symbol: order.inst_id,
        side: order.side,
        pos_side: order.pos_side,
        order_type: order.ord_type,
        price: optional_string(order.px),
        trigger_px,
        take_profit_px,
        stop_loss_px,
        size: order.sz,
        filled_size: if order.acc_fill_sz.trim().is_empty() {
            "0".to_string()
        } else {
            order.acc_fill_sz
        },
        status: order.state,
        reduce_only: order.reduce_only,
        close_on_trigger: order.close_on_trigger,
        created_at: optional_string(order.created_at).unwrap_or_default(),
        updated_at: optional_string(order.updated_at).unwrap_or_default(),
    }
}

fn convert_algo_order(order: okx::models::AlgoOrderDetail) -> OrderResponse {
    let trigger_px = optional_string(order.trigger_px.clone());
    let take_profit_px = optional_string(order.tp_trigger_px.clone()).or_else(|| {
        trigger_px
            .clone()
            .filter(|_| is_take_profit_algo_order(&order))
    });
    let stop_loss_px = optional_string(order.sl_trigger_px.clone()).or_else(|| {
        trigger_px
            .clone()
            .filter(|_| is_stop_loss_algo_order(&order))
    });
    let order_id = optional_string(order.algo_id.clone())
        .or_else(|| optional_string(order.algo_cl_ord_id.clone()))
        .unwrap_or_else(|| {
            format!(
                "{}-{}-{}",
                order.inst_id.as_deref().unwrap_or("algo"),
                order.ord_type.as_deref().unwrap_or("order"),
                order.created_at.as_deref().unwrap_or("pending")
            )
        });

    OrderResponse {
        order_id,
        symbol: optional_string(order.inst_id).unwrap_or_default(),
        side: optional_string(order.side).unwrap_or_else(|| "sell".to_string()),
        pos_side: optional_string(order.pos_side),
        order_type: optional_string(order.ord_type).unwrap_or_else(|| "algo".to_string()),
        price: optional_string(order.ord_px).or_else(|| optional_string(order.order_px)),
        trigger_px,
        take_profit_px,
        stop_loss_px,
        size: optional_string(order.sz).unwrap_or_else(|| "0".to_string()),
        filled_size: "0".to_string(),
        status: optional_string(order.state).unwrap_or_else(|| "live".to_string()),
        reduce_only: None,
        close_on_trigger: None,
        created_at: optional_string(order.created_at).unwrap_or_default(),
        updated_at: optional_string(order.updated_at).unwrap_or_default(),
    }
}

fn convert_fill(fill: okx::models::FillDetail) -> FillResponse {
    let timestamp =
        optional_string(fill.ts).unwrap_or_else(|| Utc::now().timestamp_millis().to_string());
    let order_id = optional_string(fill.ord_id).unwrap_or_default();
    let fill_id = optional_string(fill.trade_id)
        .unwrap_or_else(|| format!("{}-{}-{}", order_id, fill.inst_id, timestamp));

    FillResponse {
        fill_id,
        order_id,
        symbol: fill.inst_id,
        side: optional_string(fill.side).unwrap_or_else(|| "buy".to_string()),
        price: optional_string(fill.fill_px).unwrap_or_else(|| "0".to_string()),
        size: optional_string(fill.fill_sz).unwrap_or_else(|| "0".to_string()),
        fee: optional_string(fill.fee).unwrap_or_else(|| "0".to_string()),
        pnl: optional_string(fill.fill_pnl),
        timestamp,
    }
}

async fn fetch_algo_pending_orders(
    client: &OkxRestClient,
    symbol_filter: Option<&str>,
) -> Vec<okx::models::AlgoOrderDetail> {
    const ALGO_ORDER_TYPES: [&str; 3] = ["conditional", "oco", "trigger"];

    match client
        .get_algo_pending_orders(Some("SWAP"), symbol_filter, None)
        .await
    {
        Ok(orders) => return orders,
        Err(err) => {
            tracing::debug!(error = ?err, "failed to fetch all algo pending orders, retrying by ordType");
        }
    }

    let mut orders = Vec::new();
    for ord_type in ALGO_ORDER_TYPES {
        match client
            .get_algo_pending_orders(Some("SWAP"), symbol_filter, Some(ord_type))
            .await
        {
            Ok(mut page) => orders.append(&mut page),
            Err(err) => tracing::warn!(
                error = ?err,
                ord_type,
                "failed to fetch algo pending orders"
            ),
        }
    }
    orders
}

fn group_algo_orders_by_inst(
    orders: Vec<okx::models::AlgoOrderDetail>,
) -> HashMap<String, Vec<okx::models::AlgoOrderDetail>> {
    let mut grouped: HashMap<String, Vec<okx::models::AlgoOrderDetail>> = HashMap::new();
    for order in orders {
        if let Some(inst_id) = optional_string(order.inst_id.clone()) {
            grouped.entry(inst_id).or_default().push(order);
        }
    }
    grouped
}

fn position_take_profit_px(
    detail: &okx::models::PositionDetail,
    algo_orders: &[okx::models::AlgoOrderDetail],
) -> Option<f64> {
    parse_optional_number(detail.tp_trigger_px.clone())
        .or_else(|| {
            detail
                .close_order_algo
                .iter()
                .find_map(|order| parse_optional_number(order.tp_trigger_px.clone()))
        })
        .or_else(|| {
            algo_orders
                .iter()
                .find_map(|order| parse_optional_number(order.tp_trigger_px.clone()))
        })
        .or_else(|| infer_trigger_level_from_algo_orders(detail, algo_orders, true))
}

fn position_stop_loss_px(
    detail: &okx::models::PositionDetail,
    algo_orders: &[okx::models::AlgoOrderDetail],
) -> Option<f64> {
    parse_optional_number(detail.sl_trigger_px.clone())
        .or_else(|| {
            detail
                .close_order_algo
                .iter()
                .find_map(|order| parse_optional_number(order.sl_trigger_px.clone()))
        })
        .or_else(|| {
            algo_orders
                .iter()
                .find_map(|order| parse_optional_number(order.sl_trigger_px.clone()))
        })
        .or_else(|| infer_trigger_level_from_algo_orders(detail, algo_orders, false))
}

fn infer_trigger_level_from_algo_orders(
    detail: &okx::models::PositionDetail,
    algo_orders: &[okx::models::AlgoOrderDetail],
    want_take_profit: bool,
) -> Option<f64> {
    let avg_price = parse_optional_number(detail.avg_px.clone())?;
    let direction = infer_position_side(okx_position_size(detail), "net", &Value::Null);

    detail
        .close_order_algo
        .iter()
        .chain(algo_orders.iter())
        .find_map(|order| {
            let trigger = parse_optional_number(order.trigger_px.clone())?;
            let is_take_profit = match direction.as_str() {
                "long" => trigger >= avg_price,
                "short" => trigger <= avg_price,
                _ => is_take_profit_algo_order(order),
            };
            (is_take_profit == want_take_profit).then_some(trigger)
        })
}

fn is_take_profit_order(order: &okx::models::OrderHistoryEntry) -> bool {
    order_label(order).contains("tp")
        || order_label(order).contains("take")
        || order_label(order).contains("profit")
}

fn is_stop_loss_order(order: &okx::models::OrderHistoryEntry) -> bool {
    order_label(order).contains("sl")
        || order_label(order).contains("stop")
        || order_label(order).contains("loss")
}

fn is_take_profit_algo_order(order: &okx::models::AlgoOrderDetail) -> bool {
    algo_order_label(order).contains("tp")
        || algo_order_label(order).contains("take")
        || algo_order_label(order).contains("profit")
}

fn is_stop_loss_algo_order(order: &okx::models::AlgoOrderDetail) -> bool {
    algo_order_label(order).contains("sl")
        || algo_order_label(order).contains("stop")
        || algo_order_label(order).contains("loss")
}

fn order_label(order: &okx::models::OrderHistoryEntry) -> String {
    format!(
        "{} {} {}",
        order.cl_ord_id.as_deref().unwrap_or_default(),
        order.tag.as_deref().unwrap_or_default(),
        order.ord_type
    )
    .to_ascii_lowercase()
}

fn algo_order_label(order: &okx::models::AlgoOrderDetail) -> String {
    format!(
        "{} {} {} {}",
        order.algo_cl_ord_id.as_deref().unwrap_or_default(),
        order.cl_ord_id.as_deref().unwrap_or_default(),
        order.tag.as_deref().unwrap_or_default(),
        order.ord_type.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase()
}

fn convert_position_snapshot(record: db::PositionSnapshotRecord) -> PositionSnapshotResponse {
    let side = infer_position_side(record.size, &record.side, &record.metadata);

    PositionSnapshotResponse {
        inst_id: record.inst_id,
        pos_side: record.pos_side,
        td_mode: record.td_mode,
        side,
        size: record.size,
        avg_price: record.avg_price,
        mark_px: record.mark_px,
        margin: record.margin,
        unrealized_pnl: record.unrealized_pnl,
        take_profit_px: read_metadata_number(&record.metadata, "tpTriggerPx"),
        stop_loss_px: read_metadata_number(&record.metadata, "slTriggerPx"),
        last_trade_at: format_datetime(record.last_trade_at),
        closed_at: format_datetime(record.closed_at),
        action_kind: record.action_kind,
        entry_ord_id: record.entry_ord_id,
        exit_ord_id: record.exit_ord_id,
        metadata: record.metadata,
        updated_at: record.updated_at.to_rfc3339(),
    }
}

fn format_datetime(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|dt| dt.to_rfc3339())
}

fn infer_position_side(size: f64, fallback: &str, metadata: &Value) -> String {
    let effective_size = if size.abs() > 1e-9 {
        Some(size)
    } else {
        read_metadata_number(metadata, "pos")
    };

    match effective_size {
        Some(value) if value > 0.0 => "long".to_string(),
        Some(value) if value < 0.0 => "short".to_string(),
        _ if fallback == "long" || fallback == "short" => fallback.to_string(),
        _ => "net".to_string(),
    }
}

fn read_metadata_number(metadata: &Value, key: &str) -> Option<f64> {
    match metadata.get(key)? {
        Value::Number(value) => value.as_f64().filter(|num| num.is_finite()),
        Value::String(value) => value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|num| num.is_finite()),
        _ => None,
    }
}

async fn seed_initial_equity_from_config() -> Option<InitialEquityRecord> {
    let amount = CONFIG.initial_equity;
    tracing::info!(amount, "seeding initial equity from configuration");

    if let Err(err) = insert_initial_equity(amount).await {
        tracing::warn!(error = ?err, "failed to insert initial equity from config");
        return None;
    }

    match fetch_initial_equity().await {
        Ok(Some((stored_amount, recorded_at))) => Some(InitialEquityRecord {
            amount: format_amount(stored_amount),
            recorded_at: recorded_at.to_rfc3339(),
        }),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(error = ?err, "failed to re-fetch initial equity after seeding");
            None
        }
    }
}

async fn load_initial_equity_from_db() -> InitialEquityRecord {
    match fetch_initial_equity().await {
        Ok(Some((amount, recorded_at))) => build_initial_equity_record(amount, recorded_at),
        Ok(None) => seed_initial_equity_from_config()
            .await
            .unwrap_or_else(default_initial_equity_record),
        Err(err) => {
            tracing::warn!(error = ?err, "failed to fetch initial equity, using defaults");
            default_initial_equity_record()
        }
    }
}

async fn ensure_initial_equity_from_env(env_amount: f64) -> Result<InitialEquityRecord> {
    let current = fetch_initial_equity().await?;
    if let Some((amount, recorded_at)) = current {
        if floats_close(amount, env_amount) {
            return Ok(build_initial_equity_record(amount, recorded_at));
        }
    }

    insert_initial_equity(env_amount).await?;
    let (amount, recorded_at) = fetch_initial_equity()
        .await?
        .ok_or_else(|| anyhow!("initial_equity_missing_after_env_sync"))?;
    Ok(build_initial_equity_record(amount, recorded_at))
}

fn build_initial_equity_record(amount: f64, recorded_at: DateTime<Utc>) -> InitialEquityRecord {
    InitialEquityRecord {
        amount: format_amount(amount),
        recorded_at: recorded_at.to_rfc3339(),
    }
}

fn floats_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= INITIAL_EQUITY_TOLERANCE
}

/// 解析可选的数字字符串为 f64（过滤非有限值）
fn parse_optional_number(value: Option<String>) -> Option<f64> {
    optional_string(value)
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|v| v.is_finite())
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

/// 如果余额发生变化则记录新的余额快照
///
/// 会与上一次快照比较，如果变化超过容差阈值才记录新快照
async fn record_balance_snapshot_if_changed(
    available: f64,
    locked: f64,
    valuation: f64,
) -> anyhow::Result<()> {
    let abs_threshold = CONFIG.balance_snapshot_min_abs_change.max(0.0);
    let rel_threshold = CONFIG.balance_snapshot_min_relative_change.max(0.0);

    // 获取最近一次的余额快照
    if let Some(previous) = fetch_latest_balance_snapshot(BALANCE_ASSET).await? {
        let diff = (previous.valuation - valuation).abs();
        // 如果总估值变化小于容差阈值，则跳过记录
        if diff < BALANCE_SNAPSHOT_TOLERANCE {
            return Ok(());
        }

        let previous_abs = previous.valuation.abs();
        let relative_change = if previous_abs > f64::EPSILON {
            diff / previous_abs
        } else {
            f64::INFINITY
        };

        if diff < abs_threshold && relative_change < rel_threshold {
            return Ok(());
        }
    }

    // 插入新的余额快照到数据库
    insert_balance_snapshot(BalanceSnapshotInsert {
        asset: BALANCE_ASSET.to_string(),
        available,
        locked,
        valuation,
        source: BALANCE_SOURCE.to_string(),
    })
    .await
}

/// 定期余额快照循环任务
///
/// 每 30 秒从 OKX 获取一次账户余额，如果有变化则记录到数据库
/// 用于生成账户权益曲线图
pub async fn run_balance_snapshot_loop(state: AppState) {
    loop {
        if let Some(client) = state.okx_client.clone() {
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
        sleep(Duration::from_secs(BALANCE_SNAPSHOT_INTERVAL_SECS)).await;
    }
}
