use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::OnceLock;

use ai_core::config::CONFIG;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

mod config;
use config::load_app_config;
use okx::OkxRestClient;

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

#[derive(Clone)]
struct AppState {
    inner: Arc<RwLock<MockDataStore>>,
    okx: Option<OkxRestClient>,
}

#[derive(Debug)]
struct MockDataStore {
    tickers: HashMap<String, Ticker>,
    orderbooks: HashMap<String, OrderBook>,
    trades: HashMap<String, Vec<Trade>>,
    balances: Vec<Balance>,
    open_orders: Vec<Order>,
    fills: Vec<Fill>,
    next_order_id: u64,
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
struct Balance {
    asset: String,
    available: String,
    locked: String,
    valuation_usdt: String,
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
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaceOrderRequest {
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    order_type: String,
    price: Option<String>,
    size: String,
    time_in_force: Option<String>,
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
        .route("/account/orders/open", get(get_open_orders))
        .route("/account/orders/history", get(get_order_history))
        .route("/account/fills", get(get_fills))
        .route("/orders", post(place_order))
        .route("/orders/:order_id", delete(cancel_order))
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

    // 触发配置加载，确保 .env 生效
    let _ = &CONFIG.okx_rest_endpoint;
    let proxy_options = okx::ProxyOptions {
        http: http_proxy,
        https: https_proxy,
    };
    let app_state = AppState {
        inner: Arc::new(RwLock::new(MockDataStore::new())),
        okx: OkxRestClient::from_config_with_proxy(&CONFIG, proxy_options).ok(),
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
    tracing::info!(symbol = %symbol, "received ticker request");
    if let Some(client) = state.okx.clone() {
        match client.get_ticker(&symbol).await {
            Ok(remote) => {
                tracing::info!(symbol = %symbol, "okx ticker hit");
                let mut ticker = Ticker::from(remote);
                ticker.symbol = symbol.clone();
                return Json(ApiResponse::ok(ticker));
            }
            Err(err) => tracing::warn!(symbol = %symbol, error = ?err, "okx ticker fetch failed"),
        }
    }

    tracing::info!(symbol = %symbol, "using mock ticker");
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

async fn get_balances(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("received balances request");
    let store = state.inner.read().await;
    Json(ApiResponse::ok(store.balances.clone()))
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

async fn get_order_history(
    State(state): State<AppState>,
    Query(params): Query<SymbolOptionalQuery>,
) -> impl IntoResponse {
    tracing::info!(?params, "received order history request");
    let store = state.inner.read().await;
    let mut orders = store.open_orders.clone();

    if let Some(symbol) = params.symbol {
        orders.retain(|order| order.symbol == symbol);
    }

    if let Some(state_filter) = params.state {
        orders.retain(|order| order.status.to_string().eq_ignore_ascii_case(&state_filter));
    }

    if let Some(limit) = params.limit {
        orders.truncate(limit);
    }

    Json(ApiResponse::ok(orders))
}

async fn get_fills(
    State(state): State<AppState>,
    Query(params): Query<SymbolOptionalQuery>,
) -> impl IntoResponse {
    let store = state.inner.read().await;
    let mut fills = store.fills.clone();

    if let Some(symbol) = params.symbol {
        fills.retain(|fill| fill.symbol == symbol);
    }

    if let Some(limit) = params.limit {
        fills.truncate(limit);
    }

    Json(ApiResponse::ok(fills))
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
            timestamp: current_timestamp_minus(43_200_000),
        }];

        Self {
            tickers,
            orderbooks: HashMap::from([("BTC-USDT".into(), orderbook)]),
            trades: HashMap::from([("BTC-USDT".into(), trades)]),
            balances,
            open_orders,
            fills,
            next_order_id: 200000,
        }
    }
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
