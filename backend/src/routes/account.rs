use std::time::Duration;

use axum::{
    extract::{Json, Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::sleep;

use crate::db;
use crate::db::{
    fetch_balance_snapshots, fetch_initial_equity, fetch_latest_balance_snapshot,
    fetch_position_snapshots, insert_balance_snapshot, insert_initial_equity,
    BalanceSnapshotInsert,
};
use crate::okx::{self, OkxRestClient};
use crate::settings::CONFIG;
use crate::types::ApiResponse;
use crate::AppState;
use anyhow::Result;

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
    last_trade_at: Option<String>,
    closed_at: Option<String>,
    action_kind: Option<String>,
    entry_ord_id: Option<String>,
    exit_ord_id: Option<String>,
    metadata: Value,
    updated_at: String,
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
                snapshots.last().map(|snapshot| snapshot.recorded_at.clone())
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
    let initial = match fetch_initial_equity().await {
        Ok(Some((amount, recorded_at))) => InitialEquityRecord {
            amount: format_amount(amount),
            recorded_at: recorded_at.to_rfc3339(),
        },
        Ok(None) => seed_initial_equity_from_config().await.unwrap_or_else(default_initial_equity_record),
        Err(err) => {
            tracing::warn!(error = ?err, "failed to fetch initial equity, using defaults");
            default_initial_equity_record()
        }
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
/// 从本地 `positions` 表返回当前持仓快照（`closed_at IS NULL`）
#[axum::debug_handler]
async fn get_positions() -> Json<ApiResponse<Vec<PositionSnapshotResponse>>> {
    match fetch_position_snapshots(false, None, None).await {
        Ok(records) => {
            let snapshots: Vec<PositionSnapshotResponse> =
                records.into_iter().map(convert_position_snapshot).collect();
            Json(ApiResponse::ok(snapshots))
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to fetch current positions from database");
            Json(ApiResponse::ok(Vec::<PositionSnapshotResponse>::new()))
        }
    }
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

fn convert_position_snapshot(record: db::PositionSnapshotRecord) -> PositionSnapshotResponse {
    PositionSnapshotResponse {
        inst_id: record.inst_id,
        pos_side: record.pos_side,
        td_mode: record.td_mode,
        side: record.side,
        size: record.size,
        avg_price: record.avg_price,
        mark_px: record.mark_px,
        margin: record.margin,
        unrealized_pnl: record.unrealized_pnl,
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
