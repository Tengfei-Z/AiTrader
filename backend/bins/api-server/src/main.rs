use std::collections::HashMap;
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
    inner: Arc<RwLock<MockDataStore>>,
    okx: Option<OkxRestClient>,
    okx_simulated: Option<OkxRestClient>,
    deepseek: Option<DeepSeekClient>,
}

#[derive(Debug)]
struct MockDataStore {
    tickers: HashMap<String, Ticker>,
    orderbooks: HashMap<String, OrderBook>,
    trades: HashMap<String, Vec<Trade>>,
    balances: Vec<Balance>,
    positions: Vec<Position>,
    position_history: Vec<PositionHistory>,
    open_orders: Vec<Order>,
    fills: Vec<Fill>,
    next_order_id: u64,
    strategy_messages: Vec<StrategyMessage>,
    strategy_run_counter: u64,
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
        inner: Arc::new(RwLock::new(MockDataStore::new())),
        okx: okx_client,
        okx_simulated,
        deepseek: deepseek_client,
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
    Query(SymbolQuery { symbol, .. }): Query<SymbolQuery>,
) -> impl IntoResponse {
    debug!(symbol = %symbol, "received ticker request");
    if let Some(client) = state.okx.clone() {
        match client.get_ticker(&symbol).await {
            Ok(remote) => {
                debug!(symbol = %symbol, "okx ticker hit");
                let mut ticker = Ticker::from(remote);
                ticker.symbol = symbol.clone();
                return Json(ApiResponse::ok(ticker));
            }
            Err(err) => tracing::warn!(symbol = %symbol, error = ?err, "okx ticker fetch failed"),
        }
    }

    debug!(symbol = %symbol, "using mock ticker");
    let store = state.inner.read().await;
    let response = store
        .tickers
        .get(&symbol)
        .cloned()
        .map(ApiResponse::ok)
        .unwrap_or_else(|| ApiResponse::error(format!("symbol {symbol} not found")));

    Json(response)
}

async fn get_orderbook(
    State(state): State<AppState>,
    Query(SymbolQuery { symbol, depth, .. }): Query<SymbolQuery>,
) -> impl IntoResponse {
    tracing::info!(symbol = %symbol, "received orderbook request");
    let store = state.inner.read().await;
    let response = store
        .orderbooks
        .get(&symbol)
        .cloned()
        .map(|mut book| {
            if let Some(depth) = depth {
                book.bids.truncate(depth);
                book.asks.truncate(depth);
            }
            ApiResponse::ok(book)
        })
        .unwrap_or_else(|| ApiResponse::error(format!("symbol {symbol} not found")));

    Json(response)
}

async fn get_trades(
    State(state): State<AppState>,
    Query(SymbolQuery { symbol, limit, .. }): Query<SymbolQuery>,
) -> impl IntoResponse {
    tracing::info!(symbol = %symbol, "received trades request");
    let store = state.inner.read().await;
    let response = store
        .trades
        .get(&symbol)
        .cloned()
        .map(|mut trades| {
            if let Some(limit) = limit {
                trades.truncate(limit);
            }
            ApiResponse::ok(trades)
        })
        .unwrap_or_else(|| ApiResponse::error(format!("symbol {symbol} not found")));

    Json(response)
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

    let store = state.inner.read().await;
    Json(ApiResponse::ok(store.balances.clone()))
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

    let store = state.inner.read().await;
    Json(ApiResponse::ok(store.positions.clone()))
}

async fn get_open_orders(
    State(state): State<AppState>,
    Query(SymbolOptionalQuery { symbol, .. }): Query<SymbolOptionalQuery>,
) -> impl IntoResponse {
    tracing::info!(?symbol, "received open orders request");
    let store = state.inner.read().await;
    let mut orders: Vec<Order> = store
        .open_orders
        .iter()
        .filter(|order| order.status == OrderStatus::Open)
        .cloned()
        .collect();

    if let Some(symbol) = symbol {
        orders.retain(|order| order.symbol == symbol);
    }

    Json(ApiResponse::ok(orders))
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

    let mut fills = {
        let store = state.inner.read().await;
        store.fills.clone()
    };

    if let Some(symbol) = symbol_filter {
        fills.retain(|fill| fill.symbol == symbol);
    }

    if let Some(limit) = limit {
        fills.truncate(limit);
    }

    Json(ApiResponse::ok(fills))
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

    let mut history = {
        let store = state.inner.read().await;
        store.position_history.clone()
    };

    if let Some(symbol) = symbol_filter {
        history.retain(|item| item.symbol == symbol);
    }

    if let Some(limit) = limit {
        history.truncate(limit);
    }

    Json(ApiResponse::ok(history))
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
    let store = state.inner.read().await;
    Json(ApiResponse::ok(store.strategy_messages.clone()))
}

async fn trigger_strategy_run(State(state): State<AppState>) -> impl IntoResponse {
    let Some(deepseek_client) = state.deepseek.clone() else {
        tracing::error!("DeepSeek client not initialised");
        return Json(ApiResponse::<Vec<StrategyMessage>>::error(
            "DeepSeek 未配置或初始化失败",
        ));
    };

    let run_id = {
        let mut store = state.inner.write().await;
        store.strategy_run_counter += 1;
        store.strategy_run_counter
    };

    let parameters_schema = json!({
        "type": "object",
        "properties": {
            "include_positions": { "type": "boolean", "default": true },
            "include_history": { "type": "boolean", "default": true },
            "include_performance": { "type": "boolean", "default": true },
            "simulated_trading": { "type": "boolean", "default": false }
        },
        "required": [
            "include_positions",
            "include_history",
            "include_performance",
            "simulated_trading"
        ],
        "additionalProperties": false
    });

    let system_prompt = format!(
        "{}\n\n附加指引：当前为 Run #{run_id} 的策略执行，请首先调用工具获取账户与持仓数据，然后据此形成公开可展示的思考总结、决策与置信度。",
        DEFAULT_FUNCTION_CALL_SYSTEM_PROMPT
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
            "parameters": parameters_schema,
            "system_prompt": system_prompt
        }),
    };

    info!(
        run_id,
        "Triggering DeepSeek get_account_state function call"
    );
    let function_response = match deepseek_client.call_function(function_request).await {
        Ok(resp) => resp,
        Err(err) => {
            tracing::error!(run_id, error = ?err, "DeepSeek function call failed");
            return Json(ApiResponse::<Vec<StrategyMessage>>::error(format!(
                "DeepSeek 函数调用失败: {err}"
            )));
        }
    };

    let account_state_json = serde_json::to_string_pretty(&function_response.output)
        .unwrap_or_else(|_| function_response.output.to_string());

    info!(
        run_id,
        output_preview = %truncate_for_log(&account_state_json, 512),
        "DeepSeek tool call succeeded"
    );

    let summary_prompt = format!(
        "你是一名专业的加密货币交易 AI。以下是通过 get_account_state 工具获得的账户与持仓数据（JSON 格式）：\n{}\n\n请基于这些数据生成公开展示用的策略输出，并满足：\n1. 输出必须包含【思考总结】【决策】【置信度】三段，且每段换行分隔。\n2. 思考总结控制在 200 字以内，描述市场洞察、仓位状态与下一步计划。\n3. 决策需明确是否开仓/平仓/调整计划，如需操作请说明工具与参数。\n4. 置信度为 0-100 的整数。\n",
        account_state_json
    );

    info!(run_id, "Requesting DeepSeek summary synthesis");
    let summary_content = match deepseek_client.chat_completion(&summary_prompt).await {
        Ok(text) => {
            info!(
                run_id,
                summary_preview = %truncate_for_log(&text, 256),
                "DeepSeek summary generated"
            );
            text
        }
        Err(err) => {
            tracing::error!(run_id, error = ?err, "DeepSeek summary generation failed");
            format!(
                "【思考总结】\n未能生成总结，以下为账户数据：\n{}\n\n【决策】\n保持观望，待重新获取模型输出。\n\n【置信度】\n30",
                account_state_json
            )
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
        let mut store = state.inner.write().await;
        store.strategy_messages.push(strategy_message);
        store.strategy_messages.clone()
    };

    Json(ApiResponse::ok(messages))
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
    State(state): State<AppState>,
    Json(payload): Json<PlaceOrderRequest>,
) -> impl IntoResponse {
    let mut store = state.inner.write().await;

    let order_id = store.next_order_id.to_string();
    store.next_order_id += 1;

    let order = Order {
        order_id: order_id.clone(),
        symbol: payload.symbol.clone(),
        side: payload.side,
        order_type: payload.order_type,
        price: payload.price,
        size: payload.size,
        filled_size: "0".into(),
        status: OrderStatus::Open,
        created_at: current_timestamp(),
    };

    store.open_orders.push(order.clone());

    Json(ApiResponse::ok(PlaceOrderResponse {
        order_id,
        status: OrderStatus::Open,
    }))
}

async fn cancel_order(
    State(state): State<AppState>,
    Path(order_id): Path<String>,
) -> impl IntoResponse {
    let mut store = state.inner.write().await;
    if let Some(order) = store
        .open_orders
        .iter_mut()
        .find(|order| order.order_id == order_id)
    {
        order.status = OrderStatus::Canceled;
        return (StatusCode::OK, Json(ApiResponse::ok(order.clone())));
    }

    (
        StatusCode::NOT_FOUND,
        Json(ApiResponse::<Order>::error("订单不存在")),
    )
}

impl MockDataStore {
    fn new() -> Self {
        let mut tickers = HashMap::new();
        tickers.insert(
            "BTC-USDT".into(),
            Ticker {
                symbol: "BTC-USDT".into(),
                last: "112391.1".into(),
                bid_px: Some("112391.0".into()),
                ask_px: Some("112391.2".into()),
                high24h: Some("115590".into()),
                low24h: Some("112084.7".into()),
                vol24h: Some("8637.6433954".into()),
                timestamp: current_timestamp(),
            },
        );

        let orderbook = OrderBook {
            bids: vec![
                ("112391.0".into(), "0.52".into()),
                ("112390.5".into(), "0.35".into()),
                ("112388.0".into(), "0.71".into()),
            ],
            asks: vec![
                ("112392.0".into(), "0.40".into()),
                ("112395.0".into(), "0.22".into()),
                ("112398.5".into(), "0.19".into()),
            ],
            timestamp: current_timestamp(),
        };

        let trades = vec![
            Trade {
                trade_id: "T20241127001".into(),
                price: "112391.1".into(),
                size: "0.01".into(),
                side: "buy".into(),
                timestamp: current_timestamp_minus(30_000),
            },
            Trade {
                trade_id: "T20241127002".into(),
                price: "112390.8".into(),
                size: "0.03".into(),
                side: "sell".into(),
                timestamp: current_timestamp_minus(25_000),
            },
        ];

        let balances = vec![
            Balance {
                asset: "BTC".into(),
                available: "0.523".into(),
                locked: "0.05".into(),
                valuation_usdt: "58768.23".into(),
            },
            Balance {
                asset: "USDT".into(),
                available: "15432.5".into(),
                locked: "1500".into(),
                valuation_usdt: "16932.5".into(),
            },
        ];

        let positions = vec![
            Position {
                symbol: "BTC-USDT-SWAP".into(),
                side: "long".into(),
                entry_price: Some(108_000.0),
                current_price: Some(112_200.0),
                quantity: Some(0.35),
                leverage: Some(5.0),
                liquidation_price: Some(98_500.0),
                margin: Some(7_000.0),
                unrealized_pnl: Some(1470.0),
                entry_time: Some(current_timestamp_minus(86_400_000)),
            },
            Position {
                symbol: "ETH-USDT-SWAP".into(),
                side: "short".into(),
                entry_price: Some(3_450.0),
                current_price: Some(3_380.0),
                quantity: Some(5.2),
                leverage: Some(3.0),
                liquidation_price: Some(3_880.0),
                margin: Some(6_000.0),
                unrealized_pnl: Some(364.0),
                entry_time: Some(current_timestamp_minus(43_200_000)),
            },
        ];

        let position_history = vec![
            PositionHistory {
                symbol: "BTC-USDT-SWAP".into(),
                side: "long".into(),
                quantity: Some(0.25),
                leverage: Some(4.0),
                entry_price: Some(99_800.0),
                exit_price: Some(108_450.0),
                margin: Some(5_500.0),
                realized_pnl: Some(2150.0),
                entry_time: Some(current_timestamp_minus(259_200_000)),
                exit_time: Some(current_timestamp_minus(172_800_000)),
            },
            PositionHistory {
                symbol: "ETH-USDT-SWAP".into(),
                side: "short".into(),
                quantity: Some(3.6),
                leverage: Some(3.0),
                entry_price: Some(3_580.0),
                exit_price: Some(3_420.0),
                margin: Some(4_200.0),
                realized_pnl: Some(576.0),
                entry_time: Some(current_timestamp_minus(432_000_000)),
                exit_time: Some(current_timestamp_minus(216_000_000)),
            },
        ];

        let open_orders = vec![Order {
            order_id: "123456".into(),
            symbol: "BTC-USDT".into(),
            side: "buy".into(),
            order_type: "limit".into(),
            price: Some("110000".into()),
            size: "0.05".into(),
            filled_size: "0.02".into(),
            status: OrderStatus::PartiallyFilled,
            created_at: current_timestamp_minus(86_400_000),
        }];

        let fills = vec![Fill {
            fill_id: "F20241127001".into(),
            order_id: "123456".into(),
            symbol: "BTC-USDT".into(),
            side: "buy".into(),
            price: "109500".into(),
            size: "0.03".into(),
            fee: "0.000015".into(),
            pnl: Some("12.5".into()),
            timestamp: current_timestamp_minus(43_200_000),
        }];

        Self {
            tickers,
            orderbooks: HashMap::from([("BTC-USDT".into(), orderbook)]),
            trades: HashMap::from([("BTC-USDT".into(), trades)]),
            balances,
            positions,
            position_history,
            open_orders,
            fills,
            next_order_id: 200000,
            strategy_messages: Vec::new(),
            strategy_run_counter: 0,
        }
    }
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
