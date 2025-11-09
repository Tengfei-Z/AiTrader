use std::fs;
use std::sync::Arc;
use std::sync::OnceLock;

mod agent_client;
mod db;
mod okx;
mod server_config;
mod settings;

use crate::db::{
    fetch_initial_equity, fetch_order_history, init_database, insert_initial_equity,
    insert_strategy_summary,
};
use anyhow::{anyhow, Result};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

use agent_client::{AgentAnalysisRequest, AgentClient};
use okx::OkxRestClient;
use server_config::load_app_config;
use settings::CONFIG;

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
const DEFAULT_INITIAL_EQUITY: f64 = 122_000.0;

#[derive(Clone)]
struct AppState {
    okx_simulated: Option<OkxRestClient>,
    agent: Option<AgentClient>,
    strategy_messages: Arc<RwLock<Vec<StrategyMessage>>>,
    strategy_run_counter: Arc<RwLock<u64>>,
}

fn format_amount(value: f64) -> String {
    format!("{value:.6}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T> ApiResponse<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Ticker {
    symbol: String,
    last: String,
    bid_px: Option<String>,
    ask_px: Option<String>,
    high24h: Option<String>,
    low24h: Option<String>,
    vol24h: Option<String>,
    timestamp: String,
}

impl From<okx::models::Ticker> for Ticker {
    fn from(value: okx::models::Ticker) -> Self {
        Self {
            symbol: value.inst_id,
            last: value.last,
            bid_px: value.bid_px,
            ask_px: value.ask_px,
            high24h: value.high_24h,
            low24h: value.low_24h,
            vol24h: value.vol_24h,
            timestamp: value.ts,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OrderBook {
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Trade {
    trade_id: String,
    price: String,
    size: String,
    side: String,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StrategyMessage {
    id: String,
    role: String,
    content: String,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Balance {
    asset: String,
    available: String,
    locked: String,
    valuation_usdt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InitialEquityRecord {
    amount: String,
    recorded_at: String,
}

#[derive(Debug, Deserialize)]
struct InitialEquityPayload {
    amount: f64,
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
struct SymbolQuery {
    symbol: String,
    depth: Option<usize>,
    limit: Option<usize>,
    simulated: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SymbolOptionalQuery {
    symbol: Option<String>,
    limit: Option<usize>,
    simulated: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PlaceOrderRequest {
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    order_type: String,
    price: Option<String>,
    size: String,
}

#[derive(Debug, Serialize)]
struct PlaceOrderResponse {
    order_id: String,
    status: OrderStatus,
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/market/ticker", get(get_ticker))
        .route("/market/orderbook", get(get_orderbook))
        .route("/market/trades", get(get_trades))
        .route("/account/balances", get(get_balances))
        .route("/account/initial-equity", get(get_initial_equity))
        .route("/account/initial-equity", post(set_initial_equity))
        .route("/account/positions", get(get_positions))
        .route("/account/positions/history", get(get_positions_history))
        .route("/account/orders/open", get(get_open_orders))
        .route("/account/fills", get(get_fills))
        .route("/orders", post(place_order))
        .route("/orders/:order_id", delete(cancel_order))
        .route("/model/strategy-chat", get(get_strategy_chat))
        .route("/model/strategy-run", post(trigger_strategy_run))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let settings = load_app_config().unwrap_or_else(|err| {
        tracing::warn!("failed to load config: {err:?}, using defaults");
        Default::default()
    });
    settings.apply_runtime_env();
    let (http_proxy, https_proxy) = settings.proxy_settings();

    if let Err(err) = init_database().await {
        tracing::warn!(%err, "数据库初始化过程中出现错误");
    }

    let proxy_options = okx::ProxyOptions {
        http: http_proxy,
        https: https_proxy,
    };
    let simulated_flag = CONFIG.okx_use_simulated();
    let okx_simulated =
        match OkxRestClient::from_config_with_proxy(&CONFIG, proxy_options.clone(), simulated_flag)
        {
            Ok(client) => {
                info!(simulated = simulated_flag, "Initialized OKX client");
                Some(client)
            }
            Err(err) => {
                error!(
                    error = ?err,
                    simulated = simulated_flag,
                    "Failed to initialise OKX client"
                );
                None
            }
        };

    let agent_client = match CONFIG.agent_base_url() {
        Some(base_url) => match AgentClient::new(base_url) {
            Ok(client) => Some(client),
            Err(err) => {
                tracing::warn!(%err, "初始化 Agent 客户端失败");
                None
            }
        },
        None => {
            tracing::warn!("AGENT_BASE_URL 未配置，策略分析接口将不可用");
            None
        }
    };

    let app_state = AppState {
        okx_simulated,
        agent: agent_client,
        strategy_messages: Arc::new(RwLock::new(Vec::new())),
        strategy_run_counter: Arc::new(RwLock::new(0)),
    };
    let bind_addr = settings
        .bind_addr()
        .unwrap_or_else(|_| "0.0.0.0:3000".parse().expect("invalid default addr"));

    let router = Router::new()
        .merge(api_routes())
        .nest("/api", api_routes())
        .with_state(app_state)
        .layer(CorsLayer::new().allow_methods(Any).allow_origin(Any));

    info!("Starting API server on {bind_addr}");

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

fn init_tracing() {
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let log_dir = repo_root.join("log");

    if let Err(err) = fs::create_dir_all(&log_dir) {
        eprintln!("failed to create log directory {log_dir:?}: {err}");
    }

    let file_appender: RollingFileAppender =
        tracing_appender::rolling::daily(log_dir, "api-server.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);

    let env_filter = EnvFilter::from_default_env()
        .add_directive(Level::INFO.into())
        .add_directive("reqwest=debug".parse().unwrap()) // reqwest HTTP 详细日志
        .add_directive("hyper=debug".parse().unwrap()); // hyper HTTP 底层日志

    let fmt_stdout = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);
    let fmt_file = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    let subscriber = Registry::default()
        .with(env_filter)
        .with(fmt_stdout)
        .with(fmt_file);

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        tracing::warn!("tracing already initialised");
    }
}

async fn get_ticker(
    State(state): State<AppState>,
    Query(SymbolQuery {
        symbol, simulated, ..
    }): Query<SymbolQuery>,
) -> impl IntoResponse {
    let use_simulated = true;
    if matches!(simulated, Some(false)) {
        warn!("非模拟行情查询已被禁用，自动切换到模拟账户");
    }
    info!(symbol = %symbol, use_simulated, "received ticker request");

    // Try simulated client
    if let Some(client) = state.okx_simulated.clone() {
        match client.get_ticker(&symbol).await {
            Ok(remote) => {
                info!(
                    symbol = %symbol,
                    use_simulated,
                    last = %remote.last,
                    bid = remote.bid_px.as_deref().unwrap_or(""),
                    ask = remote.ask_px.as_deref().unwrap_or(""),
                    "okx ticker hit"
                );
                let mut ticker = Ticker::from(remote);
                ticker.symbol = symbol.clone();
                return Json(ApiResponse::ok(ticker));
            }
            Err(err) => {
                tracing::warn!(symbol = %symbol, error = ?err, use_simulated, "okx ticker fetch failed")
            }
        }
    }

    Json(ApiResponse::<Ticker>::error(format!(
        "symbol {symbol} not found"
    )))
}

async fn get_orderbook(
    _state: State<AppState>,
    Query(SymbolQuery {
        symbol,
        depth: _depth,
        ..
    }): Query<SymbolQuery>,
) -> impl IntoResponse {
    tracing::info!(symbol = %symbol, "received orderbook request");
    Json(ApiResponse::<OrderBook>::error(format!(
        "symbol {symbol} not found"
    )))
}

async fn get_trades(
    _state: State<AppState>,
    Query(SymbolQuery {
        symbol,
        limit: _limit,
        ..
    }): Query<SymbolQuery>,
) -> impl IntoResponse {
    tracing::info!(symbol = %symbol, "received trades request");
    Json(ApiResponse::<Vec<Trade>>::ok(Vec::new()))
}

async fn get_balances(
    State(state): State<AppState>,
    Query(BalancesQuery { simulated }): Query<BalancesQuery>,
) -> impl IntoResponse {
    let use_simulated = true;
    if matches!(simulated, Some(false)) {
        warn!("非模拟账户查询已被禁用，自动切换到模拟账户");
    }
    tracing::info!(use_simulated, "received balances request");

    if let Some(client) = state.okx_simulated.clone() {
        match client.get_account_balance().await {
            Ok(balance_response) => {
                if let Some(account) = balance_response.data.into_iter().next() {
                    let mut account_value = 0.0_f64;
                    let mut available_cash = 0.0_f64;

                    for detail in account.details {
                        if detail.ccy.eq_ignore_ascii_case("USDT") {
                            if let Some(eq) = parse_optional_number(detail.eq.clone()) {
                                account_value += eq;
                            }
                            if let Some(avail) = parse_optional_number(detail.avail_bal.clone()) {
                                available_cash += avail;
                            }
                        }
                    }

                    let locked = (account_value - available_cash).max(0.0);

                    let balances = vec![Balance {
                        asset: "USDT".into(),
                        available: format_amount(available_cash),
                        locked: format_amount(locked),
                        valuation_usdt: format_amount(account_value),
                    }];
                    tracing::info!(
                        use_simulated,
                        available = available_cash,
                        locked,
                        valuation = account_value,
                        "okx balances parsed"
                    );
                    return Json(ApiResponse::ok(balances));
                } else {
                    tracing::warn!(use_simulated, "OKX balance response contained no data");
                }
            }
            Err(err) => {
                tracing::warn!(use_simulated, error = ?err, "failed to fetch OKX balances");
            }
        }
    }

    Json(ApiResponse::ok(Vec::<Balance>::new()))
}

async fn get_initial_equity() -> impl IntoResponse {
    match fetch_initial_equity().await {
        Ok(Some((amount, recorded_at))) => Json(ApiResponse::<Option<InitialEquityRecord>>::ok(
            Some(InitialEquityRecord {
                amount: format_amount(amount),
                recorded_at: recorded_at.to_rfc3339(),
            }),
        )),
        Ok(None) => Json(ApiResponse::<Option<InitialEquityRecord>>::ok(Some(
            default_initial_equity_record(),
        ))),
        Err(err) => {
            warn!(error = ?err, "failed to read initial equity");
            Json(ApiResponse::<Option<InitialEquityRecord>>::error(
                "无法获取初始资金",
            ))
        }
    }
}

async fn set_initial_equity(Json(payload): Json<InitialEquityPayload>) -> impl IntoResponse {
    if payload.amount < 0.0 {
        return Json(ApiResponse::<Option<InitialEquityRecord>>::error(
            "初始资金不能为负值",
        ));
    }

    if let Err(err) = insert_initial_equity(payload.amount).await {
        warn!(error = ?err, "failed to write initial equity");
        return Json(ApiResponse::<Option<InitialEquityRecord>>::error(
            "无法保存初始资金",
        ));
    }

    match fetch_initial_equity().await {
        Ok(Some((amount, recorded_at))) => Json(ApiResponse::<Option<InitialEquityRecord>>::ok(
            Some(InitialEquityRecord {
                amount: format_amount(amount),
                recorded_at: recorded_at.to_rfc3339(),
            }),
        )),
        Ok(None) => Json(ApiResponse::<Option<InitialEquityRecord>>::ok(Some(
            default_initial_equity_record(),
        ))),
        Err(err) => {
            warn!(error = ?err, "failed to read initial equity after update");
            Json(ApiResponse::<Option<InitialEquityRecord>>::error(
                "保存后无法加载初始资金",
            ))
        }
    }
}

fn default_initial_equity_record() -> InitialEquityRecord {
    InitialEquityRecord {
        amount: format_amount(DEFAULT_INITIAL_EQUITY),
        recorded_at: Utc::now().to_rfc3339(),
    }
}

async fn get_positions(
    State(state): State<AppState>,
    Query(BalancesQuery { simulated }): Query<BalancesQuery>,
) -> impl IntoResponse {
    let use_simulated = true;
    if matches!(simulated, Some(false)) {
        warn!("非模拟仓位查询已被禁用，自动切换到模拟账户");
    }
    tracing::info!(use_simulated, "received positions request");

    if let Some(client) = state.okx_simulated.clone() {
        match client.get_positions(None).await {
            Ok(position_details) => {
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
                            take_profit_trigger: parse_optional_number(
                                detail.tp_trigger_px.clone(),
                            ),
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
            }
            Err(err) => {
                tracing::warn!(use_simulated, error = ?err, "failed to fetch OKX positions");
            }
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
    let use_simulated = true;
    let symbol_filter = params.symbol.clone();
    let limit = params.limit;

    tracing::info!(
        ?symbol_filter,
        use_simulated,
        limit,
        "received fills request"
    );

    if matches!(params.simulated, Some(false)) {
        warn!("非模拟成交查询已被禁用，自动切换到模拟账户");
    }

    if let Some(client) = state.okx_simulated.clone() {
        match client.get_fills(symbol_filter.as_deref(), limit).await {
            Ok(remote_fills) => {
                let mut fills: Vec<Fill> = remote_fills.into_iter().map(convert_okx_fill).collect();
                if let Some(limit) = limit {
                    fills.truncate(limit);
                }
                return Json(ApiResponse::ok(fills));
            }
            Err(err) => {
                tracing::warn!(use_simulated, error = ?err, "failed to fetch OKX fills");
            }
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

async fn get_strategy_chat(State(state): State<AppState>) -> impl IntoResponse {
    let messages = state.strategy_messages.read().await.clone();
    Json(ApiResponse::ok(messages))
}

async fn trigger_strategy_run(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("HTTP POST /model/strategy-run invoked from UI");

    let Some(agent_client) = state.agent.clone() else {
        tracing::error!("Agent client not initialised");
        return Json(ApiResponse::<()>::error("AI Agent 未配置或初始化失败"));
    };

    let run_id = {
        let mut counter = state.strategy_run_counter.write().await;
        *counter += 1;
        *counter
    };

    info!(run_id, "Triggering agent strategy analysis");

    let session_id = format!("strategy-auto-{run_id}");
    tracing::info!(
        run_id,
        session_id = %session_id,
        "Dispatching agent analysis request"
    );

    let background_state = state.clone();
    tokio::spawn(async move {
        if let Err(err) = run_strategy_job(background_state, agent_client, run_id, session_id).await
        {
            warn!(run_id, %err, "Strategy analysis task failed");
        }
    });

    Json(ApiResponse::ok(()))
}

async fn run_strategy_job(
    state: AppState,
    agent_client: AgentClient,
    run_id: u64,
    session_id: String,
) -> Result<()> {
    let timeout_budget = Duration::from_secs(60);
    let request = AgentAnalysisRequest {
        session_id: session_id.clone(),
    };

    let response = match timeout(timeout_budget, agent_client.analysis(request)).await {
        Err(_) => {
            tracing::error!(run_id, "Agent analysis timed out");
            return Err(anyhow!("agent_analysis_timeout"));
        }
        Ok(result) => match result {
            Ok(resp) => resp,
            Err(err) => {
                tracing::error!(run_id, error = %err, "Agent analysis failed");
                return Err(err);
            }
        },
    };

    info!(
        run_id,
        session_id = %response.session_id,
        instrument_id = %response.instrument_id,
        analysis_type = %response.analysis_type,
        completed_at = %response.created_at,
        summary_preview = %truncate_for_log(&response.summary, 256),
        suggestions = response.suggestions.len(),
        "Agent analysis completed"
    );

    let mut content = format!("【市场分析】\n{}\n", response.summary);
    if !response.suggestions.is_empty() {
        content.push_str("\n【策略建议】\n");
        for suggestion in &response.suggestions {
            content.push_str("- ");
            content.push_str(suggestion);
            content.push('\n');
        }
    }

    let strategy_message = StrategyMessage {
        id: format!(
            "strategy-{}-{}",
            run_id,
            chrono::Utc::now().timestamp_millis()
        ),
        role: "assistant".into(),
        created_at: current_timestamp_iso(),
        summary: Some(format!("第 {} 次策略执行", run_id)),
        tags: Some(vec!["auto-run".into(), "agent".into()]),
        content,
    };

    tracing::debug!(run_id, "Appending strategy message to shared state");

    if let Err(err) = insert_strategy_summary(&response.session_id, &response.summary, None).await {
        warn!(run_id, %err, "写入策略摘要到数据库失败");
    }

    {
        let mut msgs = state.strategy_messages.write().await;
        msgs.push(strategy_message);
    }

    tracing::info!(
        run_id,
        "Strategy run completed and stored in background task"
    );
    Ok(())
}

fn truncate_for_log(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }

    text.chars().take(max_len).collect::<String>() + "…"
}

fn parse_optional_number(value: Option<String>) -> Option<f64> {
    optional_string(value)
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

async fn place_order(
    _state: State<AppState>,
    Json(payload): Json<PlaceOrderRequest>,
) -> impl IntoResponse {
    info!(
        symbol = %payload.symbol,
        side = %payload.side,
        order_type = %payload.order_type,
        price = payload.price.as_deref().unwrap_or("market"),
        size = %payload.size,
        "Received simulated place-order request; feature not implemented"
    );

    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::<PlaceOrderResponse>::error(
            "下单功能仅在接入真实交易所 API 时可用",
        )),
    )
}

async fn cancel_order(_state: State<AppState>, Path(_order_id): Path<String>) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::<Order>::error(
            "取消订单仅在接入真实交易所 API 时可用",
        )),
    )
}

fn current_timestamp_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderStatus::Open => write!(f, "open"),
            OrderStatus::PartiallyFilled => write!(f, "partially_filled"),
            OrderStatus::Filled => write!(f, "filled"),
            OrderStatus::Canceled => write!(f, "canceled"),
        }
    }
}
#[derive(Debug, Deserialize, Default)]
struct BalancesQuery {
    simulated: Option<bool>,
}
