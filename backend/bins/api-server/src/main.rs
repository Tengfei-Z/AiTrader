use std::fs;
use std::sync::Arc;
use std::sync::OnceLock;

use ai_core::{config::CONFIG, db::init_database};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, info, warn, Level};
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

mod config;
use config::load_app_config;
use deepseek::{
    DeepSeekClient, FunctionCallRequest, FunctionCaller, DEFAULT_FUNCTION_CALL_SYSTEM_PROMPT,
};
use mcp_adapter::account::{fetch_account_state, AccountStateRequest};
use okx::OkxRestClient;

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

#[derive(Clone)]
struct AppState {
    okx: Option<OkxRestClient>,
    okx_simulated: Option<OkxRestClient>,
    deepseek: Option<DeepSeekClient>,
    strategy_messages: Arc<RwLock<Vec<StrategyMessage>>>,
    strategy_run_counter: Arc<RwLock<u64>>,
    next_order_id: Arc<RwLock<u64>>, // reserved for future local bookkeeping
    last_run_status: Arc<RwLock<Option<StrategyRunStatus>>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StrategyOutcome {
    Ok,
    TimeoutStage1,
    TimeoutStage2,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StrategyRunStatus {
    run_id: u64,
    started_at: String,
    ended_at: Option<String>,
    stage1_elapsed_ms: Option<u128>,
    stage2_elapsed_ms: Option<u128>,
    outcome: StrategyOutcome,
    error: Option<String>,
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/market/ticker", get(get_ticker))
        .route("/market/orderbook", get(get_orderbook))
        .route("/market/trades", get(get_trades))
        .route("/account/balances", get(get_balances))
        .route("/account/positions", get(get_positions))
        .route("/account/positions/history", get(get_positions_history))
        .route("/account/orders/open", get(get_open_orders))
        .route("/account/fills", get(get_fills))
        .route("/orders", post(place_order))
        .route("/orders/:order_id", delete(cancel_order))
        .route("/model/strategy-chat", get(get_strategy_chat))
        .route("/model/strategy-run", post(trigger_strategy_run))
        .route("/model/strategy-status", get(get_strategy_status))
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

    // 触发配置加载，确保 .env 生效
    let _ = &CONFIG.okx_rest_endpoint;
    let proxy_options = okx::ProxyOptions {
        http: http_proxy,
        https: https_proxy,
    };
    let okx_client = None;
    let okx_simulated =
        OkxRestClient::from_config_simulated_with_proxy(&CONFIG, proxy_options.clone()).ok();

    let deepseek_client = match DeepSeekClient::from_app_config(&CONFIG) {
        Ok(client) => Some(client),
        Err(err) => {
            tracing::warn!(%err, "初始化 DeepSeek 客户端失败");
            None
        }
    };

    let app_state = AppState {
        okx: okx_client,
        okx_simulated,
        deepseek: deepseek_client,
        strategy_messages: Arc::new(RwLock::new(Vec::new())),
        strategy_run_counter: Arc::new(RwLock::new(0)),
        next_order_id: Arc::new(RwLock::new(1)),
        last_run_status: Arc::new(RwLock::new(None)),
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
    let log_dir = std::path::Path::new("logs");
    if let Err(err) = fs::create_dir_all(log_dir) {
        eprintln!("failed to create log directory {log_dir:?}: {err}");
    }

    let file_appender: RollingFileAppender =
        tracing_appender::rolling::daily(log_dir, "api-server.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);

    let env_filter = EnvFilter::from_default_env().add_directive(Level::INFO.into());

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
    Query(SymbolQuery { symbol, simulated, .. }): Query<SymbolQuery>,
) -> impl IntoResponse {
    let use_simulated = true;
    if matches!(simulated, Some(false)) {
        warn!("非模拟行情查询已被禁用，自动切换到模拟账户");
    }
    debug!(symbol = %symbol, use_simulated, "received ticker request");

    // Try simulated client
    if let Some(client) = state.okx_simulated.clone() {
        match client.get_ticker(&symbol).await {
            Ok(remote) => {
                debug!(symbol = %symbol, use_simulated, "okx ticker hit");
                let mut ticker = Ticker::from(remote);
                ticker.symbol = symbol.clone();
                return Json(ApiResponse::ok(ticker));
            }
            Err(err) => tracing::warn!(symbol = %symbol, error = ?err, use_simulated, "okx ticker fetch failed"),
        }
    }

    Json(ApiResponse::<Ticker>::error(format!("symbol {symbol} not found")))
}

async fn get_orderbook(
    _state: State<AppState>,
    Query(SymbolQuery { symbol, depth: _depth, .. }): Query<SymbolQuery>,
) -> impl IntoResponse {
    tracing::info!(symbol = %symbol, "received orderbook request");
    Json(ApiResponse::<OrderBook>::error(format!("symbol {symbol} not found")))
}

async fn get_trades(
    _state: State<AppState>,
    Query(SymbolQuery { symbol, limit: _limit, .. }): Query<SymbolQuery>,
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
        let mut request = AccountStateRequest::default();
        request.simulated_trading = use_simulated;

        match fetch_account_state(&client, &request).await {
            Ok(account_state) => {
                let locked = (account_state.account_value - account_state.available_cash).max(0.0);
                let balances = vec![Balance {
                    asset: "USDT".into(),
                    available: format_amount(account_state.available_cash),
                    locked: format_amount(locked),
                    valuation_usdt: format_amount(account_state.account_value),
                }];
                return Json(ApiResponse::ok(balances));
            }
            Err(err) => {
                tracing::warn!(use_simulated, error = ?err, "failed to fetch OKX balances");
            }
        }
    }

    Json(ApiResponse::ok(Vec::<Balance>::new()))
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
        let mut request = AccountStateRequest::default();
        request.simulated_trading = use_simulated;
        request.include_history = false;
        request.include_performance = false;

        match fetch_account_state(&client, &request).await {
            Ok(account_state) => {
                let positions: Vec<Position> = account_state
                    .active_positions
                    .into_iter()
                    .map(|pos| Position {
                        symbol: pos.coin,
                        side: pos.side,
                        entry_price: pos.entry_price,
                        current_price: pos.current_price,
                        quantity: pos.quantity,
                        leverage: pos.leverage,
                        liquidation_price: pos.liquidation_price,
                        margin: pos.margin,
                        unrealized_pnl: pos.unrealized_pnl,
                        entry_time: pos.entry_time,
                    })
                    .collect();
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
    Query(SymbolOptionalQuery { symbol: _symbol, .. }): Query<SymbolOptionalQuery>,
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
        "received positions history request"
    );

    if matches!(params.simulated, Some(false)) {
        warn!("非模拟持仓历史查询已被禁用，自动切换到模拟账户");
    }

    if let Some(client) = state.okx_simulated.clone() {
        match client
            .get_positions_history(symbol_filter.as_deref(), limit)
            .await
        {
            Ok(remote_history) => {
                let mut history: Vec<PositionHistory> = remote_history
                    .into_iter()
                    .map(convert_okx_position_history)
                    .collect();

                if let Some(symbol) = symbol_filter.as_ref() {
                    history.retain(|item| item.symbol == *symbol);
                }

                if let Some(limit) = limit {
                    history.truncate(limit);
                }

                return Json(ApiResponse::ok(history));
            }
            Err(err) => {
                tracing::warn!(
                    use_simulated,
                    error = ?err,
                    "failed to fetch OKX historical positions"
                );
            }
        }
    }

    Json(ApiResponse::ok(Vec::<PositionHistory>::new()))
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

fn convert_okx_position_history(detail: okx::models::PositionHistoryDetail) -> PositionHistory {
    let okx::models::PositionHistoryDetail {
        inst_id,
        pos_side,
        close_pos,
        open_avg_px,
        close_avg_px,
        lever,
        margin,
        pnl,
        pnl_ratio: _,
        c_time,
        u_time,
    } = detail;

    PositionHistory {
        symbol: inst_id,
        side: pos_side
            .map(|value| value.to_lowercase())
            .unwrap_or_else(|| "net".to_string()),
        quantity: parse_optional_number(close_pos),
        leverage: parse_optional_number(lever),
        entry_price: parse_optional_number(open_avg_px),
        exit_price: parse_optional_number(close_avg_px),
        margin: parse_optional_number(margin),
        realized_pnl: parse_optional_number(pnl),
        entry_time: optional_string(c_time),
        exit_time: optional_string(u_time),
    }
}

async fn get_strategy_chat(State(state): State<AppState>) -> impl IntoResponse {
    let messages = state.strategy_messages.read().await.clone();
    Json(ApiResponse::ok(messages))
}

async fn trigger_strategy_run(State(state): State<AppState>) -> impl IntoResponse {
    let Some(deepseek_client) = state.deepseek.clone() else {
        tracing::error!("DeepSeek client not initialised");
        return Json(ApiResponse::<Vec<StrategyMessage>>::error(
            "DeepSeek 未配置或初始化失败",
        ));
    };

    let run_id = {
        let mut counter = state.strategy_run_counter.write().await;
        *counter += 1;
        *counter
    };

    let system_prompt = format!(
        "{}\n\nRun #{} 规则（精简）：\n- 仅分析与交易 BTC 永续：BTC-USDT-SWAP（不涉现货）。\n- 建议下单默认标的：BTC-USDT-SWAP。\n- 先取账户/仓位，再给结论；输出含：思考、决策、置信度。",
        DEFAULT_FUNCTION_CALL_SYSTEM_PROMPT,
        run_id
    );

    let function_request = FunctionCallRequest {
        function: "get_account_state".to_string(),
        arguments: json!({
            "include_positions": true,
            "include_history": true,
            "include_performance": true,
            "simulated_trading": true
        }),
        metadata: json!({
            "source": "api-server",
            "description": "Retrieve aggregated OKX account snapshot for strategy engine.",
            "system_prompt": system_prompt
        }),
    };

    info!(
        run_id,
        "Triggering DeepSeek get_account_state function call"
    );
    // Global deadline budget for the whole run
    let budget_total = Duration::from_secs(20);
    let start_time = Instant::now();
    let started_at_iso = current_timestamp_iso();
    {
        let mut status = state.last_run_status.write().await;
        *status = Some(StrategyRunStatus {
            run_id,
            started_at: started_at_iso.clone(),
            ended_at: None,
            stage1_elapsed_ms: None,
            stage2_elapsed_ms: None,
            outcome: StrategyOutcome::Ok,
            error: None,
        });
    }

    // Stage 1: function call with remaining budget (up to 10s)
    let mut remaining = budget_total
        .checked_sub(start_time.elapsed())
        .unwrap_or_else(|| Duration::from_secs(0));
    let stage1_timeout = remaining.min(Duration::from_secs(10));

    let function_response = match timeout(stage1_timeout, deepseek_client.call_function(function_request)).await {
        Err(_) => {
            tracing::error!(run_id, "DeepSeek function call timed out");
            let mut status = state.last_run_status.write().await;
            if let Some(s) = status.as_mut() {
                s.stage1_elapsed_ms = Some(start_time.elapsed().as_millis());
                s.ended_at = Some(current_timestamp_iso());
                s.outcome = StrategyOutcome::TimeoutStage1;
                s.error = Some("function_call_timeout".into());
            }
            return Json(ApiResponse::<Vec<StrategyMessage>>::error(
                "DeepSeek 函数调用超时"
            ));
        }
        Ok(result) => match result {
        Ok(resp) => resp,
        Err(err) => {
            tracing::error!(run_id, error = ?err, "DeepSeek function call failed");
            let mut status = state.last_run_status.write().await;
            if let Some(s) = status.as_mut() {
                s.stage1_elapsed_ms = Some(start_time.elapsed().as_millis());
                s.ended_at = Some(current_timestamp_iso());
                s.outcome = StrategyOutcome::Error;
                s.error = Some(format!("function_call_error: {err}"));
            }
            return Json(ApiResponse::<Vec<StrategyMessage>>::error(format!(
                "DeepSeek 函数调用失败: {err}"
            )));
        }
    }
    };
    {
        let mut status = state.last_run_status.write().await;
        if let Some(s) = status.as_mut() {
            s.stage1_elapsed_ms = Some(start_time.elapsed().as_millis());
        }
    }

    // Focus DeepSeek analysis on BTC-only data
    let focus_coin = "BTC";

    let mut filtered_output = function_response.output.clone();
    if let serde_json::Value::Object(ref mut map) = filtered_output {
        // Helper to check if a JSON value has coin/symbol fields containing focus_coin
        let contains_focus_coin = |value: &serde_json::Value| -> bool {
            let focus = focus_coin.to_uppercase();
            match value {
                serde_json::Value::Object(obj) => {
                    let coin = obj.get("coin").and_then(|v| v.as_str()).unwrap_or("");
                    let symbol = obj.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                    coin.to_uppercase().contains(&focus) || symbol.to_uppercase().contains(&focus)
                }
                _ => false,
            }
        };

        // Filter balances to BTC and USDT only if balances exist
        if let Some(balances) = map.get_mut("balances").and_then(|v| v.as_array_mut()) {
            balances.retain(|item| {
                let asset = item
                    .get("asset")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_uppercase();
                asset == focus_coin || asset == "USDT"
            });
        }

        // Filter active positions arrays by focus coin
        if let Some(positions) = map
            .get_mut("active_positions")
            .and_then(|v| v.as_array_mut())
        {
            positions.retain(|item| contains_focus_coin(item));
        }

        // Filter positions history variants by focus coin
        if let Some(history) = map
            .get_mut("positions_history")
            .and_then(|v| v.as_array_mut())
        {
            history.retain(|item| contains_focus_coin(item));
        }
        if let Some(history) = map
            .get_mut("historical_positions")
            .and_then(|v| v.as_array_mut())
        {
            history.retain(|item| contains_focus_coin(item));
        }

        // Optional: filter performance metrics per-coin if structured as an object of coins
        if let Some(perf) = map.get_mut("performance").and_then(|v| v.as_object_mut()) {
            perf.retain(|key, _| {
                let k = key.to_uppercase();
                k == focus_coin || k == "USDT" || k == "TOTAL"
            });
        }
    }

    // Use compact JSON to reduce tokens
    let account_state_json = serde_json::to_string(&filtered_output)
        .unwrap_or_else(|_| filtered_output.to_string());

    info!(
        run_id,
        output_preview = %truncate_for_log(&account_state_json, 512),
        "DeepSeek tool call succeeded"
    );

    let summary_prompt = format!(
        "仅围绕 BTC 永续（BTC-USDT-SWAP）输出；不涉现货。账户数据（JSON）：\n{}\n\n请按以下格式简洁输出：\n【思考总结】≤200字；只谈 BTC 合约行情与仓位要点。\n【决策】是否开/平/调整；默认标的 BTC-USDT-SWAP；给关键参数。\n【置信度】0-100。",
        account_state_json
    );

    info!(run_id, "Requesting DeepSeek summary synthesis");
    // Stage 2: summary with remaining budget (up to 8s)
    remaining = budget_total
        .checked_sub(start_time.elapsed())
        .unwrap_or_else(|| Duration::from_secs(0));
    let stage2_timeout = if remaining.is_zero() { Duration::from_secs(1) } else { remaining.min(Duration::from_secs(8)) };

    let summary_start = Instant::now();
    let summary_content = match timeout(stage2_timeout, deepseek_client.chat_completion(&summary_prompt)).await {
        Err(_) => {
            tracing::error!(run_id, "DeepSeek summary generation timed out");
            let mut status = state.last_run_status.write().await;
            if let Some(s) = status.as_mut() {
                s.stage2_elapsed_ms = Some(summary_start.elapsed().as_millis());
                s.ended_at = Some(current_timestamp_iso());
                s.outcome = StrategyOutcome::TimeoutStage2;
                s.error = Some("summary_timeout".into());
            }
            format!(
                "【思考总结】\n未能生成总结（超时），以下为账户数据：\n{}\n\n【决策】\n保持观望，待重新获取模型输出。\n\n【置信度】\n30",
                account_state_json
            )
        }
        Ok(result) => match result {
        Ok(text) => {
            info!(
                run_id,
                summary_preview = %truncate_for_log(&text, 256),
                "DeepSeek summary generated"
            );
            let mut status = state.last_run_status.write().await;
            if let Some(s) = status.as_mut() {
                s.stage2_elapsed_ms = Some(summary_start.elapsed().as_millis());
                s.ended_at = Some(current_timestamp_iso());
                s.outcome = StrategyOutcome::Ok;
                s.error = None;
            }
            text
        }
        Err(err) => {
            tracing::error!(run_id, error = ?err, "DeepSeek summary generation failed");
            let mut status = state.last_run_status.write().await;
            if let Some(s) = status.as_mut() {
                s.stage2_elapsed_ms = Some(summary_start.elapsed().as_millis());
                s.ended_at = Some(current_timestamp_iso());
                s.outcome = StrategyOutcome::Error;
                s.error = Some(format!("summary_error: {err}"));
            }
            format!(
                "【思考总结】\n未能生成总结，以下为账户数据：\n{}\n\n【决策】\n保持观望，待重新获取模型输出。\n\n【置信度】\n30",
                account_state_json
            )
        }
    }
    };

    let strategy_message = StrategyMessage {
        id: format!(
            "strategy-{}-{}",
            run_id,
            chrono::Utc::now().timestamp_millis()
        ),
        role: "assistant".into(),
        created_at: current_timestamp_iso(),
        summary: Some(format!("第 {} 次策略执行", run_id)),
        tags: Some(vec!["auto-run".into(), "deepseek".into()]),
        content: summary_content,
    };

    let messages = {
        let mut msgs = state.strategy_messages.write().await;
        msgs.push(strategy_message);
        msgs.clone()
    };

    Json(ApiResponse::ok(messages))
}

async fn get_strategy_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.last_run_status.read().await.clone();
    Json(ApiResponse::ok(status))
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
    Json(_payload): Json<PlaceOrderRequest>,
) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::<PlaceOrderResponse>::error(
            "下单功能仅在接入真实交易所 API 时可用",
        )),
    )
}

async fn cancel_order(
    _state: State<AppState>,
    Path(_order_id): Path<String>,
) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::<Order>::error("取消订单仅在接入真实交易所 API 时可用")),
    )
}

fn current_timestamp_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn current_timestamp() -> String {
    chrono::Utc::now().timestamp_millis().to_string()
}

fn current_timestamp_minus(ms: i64) -> String {
    (chrono::Utc::now().timestamp_millis() - ms).to_string()
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
