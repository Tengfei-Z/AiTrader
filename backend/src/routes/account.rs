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

/// 当前持仓信息
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Position {
    /// 交易对符号（如 BTC-USDT-SWAP）
    symbol: String,
    /// 持仓方向（long/short/net）
    side: String,
    /// 开仓价格
    entry_price: Option<f64>,
    /// 当前价格（标记价格或最新价）
    current_price: Option<f64>,
    /// 持仓数量
    quantity: Option<f64>,
    /// 杠杆倍数
    leverage: Option<f64>,
    /// 强平价格
    liquidation_price: Option<f64>,
    /// 保证金
    margin: Option<f64>,
    /// 未实现盈亏
    unrealized_pnl: Option<f64>,
    /// 开仓时间
    entry_time: Option<String>,
    /// 止盈触发价格
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_trigger: Option<f64>,
    /// 止盈委托价格
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_price: Option<f64>,
    /// 止盈触发类型
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_type: Option<String>,
    /// 止损触发价格
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_trigger: Option<f64>,
    /// 止损委托价格
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_price: Option<f64>,
    /// 止损触发类型
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_type: Option<String>,
}

/// 历史持仓记录
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PositionHistory {
    /// 交易对符号
    symbol: String,
    /// 持仓方向
    side: String,
    /// 持仓数量
    quantity: Option<f64>,
    /// 杠杆倍数
    leverage: Option<f64>,
    /// 开仓价格
    entry_price: Option<f64>,
    /// 平仓价格
    exit_price: Option<f64>,
    /// 保证金
    margin: Option<f64>,
    /// 已实现盈亏
    realized_pnl: Option<f64>,
    /// 开仓时间
    entry_time: Option<String>,
    /// 平仓时间
    exit_time: Option<String>,
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
        Ok(None) => default_initial_equity_record(),
        Err(_) => default_initial_equity_record(),
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
/// 从 OKX 交易所实时获取当前所有持仓信息，仅返回 USDT 交易对的持仓
async fn get_positions(State(state): State<AppState>) -> impl IntoResponse {
    let use_simulated = CONFIG.okx_use_simulated();
    tracing::info!(use_simulated, "received positions request");

    if let Some(client) = state.okx_client.clone() {
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

/// 获取历史持仓记录
/// 
/// 从数据库读取已平仓的历史持仓信息，支持按交易对过滤
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

/// 处理可选字符串，去除空白并返回 None（如果为空）
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

/// 返回字符串或默认值
fn string_or_default(value: Option<String>, default: &str) -> String {
    optional_string(value).unwrap_or_else(|| default.to_string())
}

/// 将数据库订单历史记录转换为 PositionHistory 格式
fn convert_order_history_record(record: db::OrderHistoryRecord) -> PositionHistory {
    // 从 metadata JSON 中提取平仓价格
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
    // 从 metadata 中提取保证金
    let margin = record
        .metadata
        .get("margin_usdt")
        .and_then(|value| value.as_f64());
    // 从 metadata 中提取已实现盈亏
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

/// 解析毫秒时间戳为 ISO 8601 字符串
fn parse_timestamp_millis(value: Option<String>) -> Option<String> {
    let millis = value?.parse::<i64>().ok()?;
    chrono::Utc
        .timestamp_millis_opt(millis)
        .single()
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// 解析可选的数字字符串为 f64（过滤非有限值）
fn parse_optional_number(value: Option<String>) -> Option<f64> {
    optional_string(value)
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

/// 如果余额发生变化则记录新的余额快照
/// 
/// 会与上一次快照比较，如果变化超过容差阈值才记录新快照
async fn record_balance_snapshot_if_changed(
    available: f64,
    locked: f64,
    valuation: f64,
) -> anyhow::Result<()> {
    // 获取最近一次的余额快照
    if let Some(previous) = fetch_latest_balance_snapshot(BALANCE_ASSET).await? {
        // 如果总估值变化小于容差阈值，则跳过记录
        if (previous.valuation - valuation).abs() < BALANCE_SNAPSHOT_TOLERANCE {
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
/// 每 5 秒从 OKX 获取一次账户余额，如果有变化则记录到数据库
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
        sleep(Duration::from_secs(5)).await;
    }
}
