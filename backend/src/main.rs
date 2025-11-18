use std::{
    fs,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::sync::Notify;
use tokio::time::Instant;

mod agent_subscriber;
mod db;
mod okx;
mod order_sync;
mod routes;
mod server_config;
mod settings;
mod strategy_trigger;
mod types;

use crate::agent_subscriber::{
    is_analysis_busy_error, is_websocket_uninitialized_error, run_agent_events_listener,
    trigger_analysis,
};
use crate::db::init_database;
use crate::okx::OkxRestClient;
use crate::routes::{api_routes, run_balance_snapshot_loop};
use crate::server_config::load_app_config;
use crate::settings::CONFIG;
use crate::strategy_trigger::{PriceDeltaSnapshot, TriggerSource};
use anyhow::Result;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

#[derive(Clone)]
struct AppState {
    okx_client: Option<OkxRestClient>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let settings = load_app_config().unwrap_or_else(|err| {
        tracing::warn!("failed to load config: {err:?}, using defaults");
        Default::default()
    });
    settings.apply_runtime_env();
    let (http_proxy, https_proxy) = settings.proxy_settings();

    if let Err(err) = init_database(CONFIG.should_reset_database()).await {
        warn!(%err, "数据库初始化过程中出现错误");
    }

    let proxy_options = okx::ProxyOptions {
        http: http_proxy,
        https: https_proxy,
    };
    let okx_client = match OkxRestClient::from_config_with_proxy(&CONFIG, proxy_options.clone()) {
        Ok(client) => {
            info!("Initialized OKX client");
            Some(client)
        }
        Err(err) => {
            tracing::error!(error = ?err, "Failed to initialise OKX client");
            None
        }
    };

    let app_state = AppState {
        okx_client: okx_client.clone(),
    };

    let background_state = app_state.clone();
    tokio::spawn(async move { run_balance_snapshot_loop(background_state).await });
    let sync_client = app_state.okx_client.clone();
    order_sync::init_client(sync_client.clone());
    tokio::spawn(async { run_agent_events_listener().await });
    tokio::spawn(async move {
        order_sync::run_periodic_position_sync().await;
    });
    if CONFIG.strategy_schedule_enabled() || CONFIG.strategy_vol_trigger_enabled() {
        let interval = Duration::from_secs(CONFIG.strategy_schedule_interval_secs());
        let scheduler_client = app_state.okx_client.clone();
        tokio::spawn(async move {
            run_strategy_scheduler_loop(interval, scheduler_client).await;
        });
    }

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
        .add_directive("reqwest=info".parse().unwrap())
        .add_directive("hyper=info".parse().unwrap());

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

const WS_RETRY_DELAY: Duration = Duration::from_secs(5);
const WS_RETRY_MAX_ATTEMPTS: usize = 3;
const VOLATILITY_POLL_INTERVAL: Duration = Duration::from_secs(5);

async fn run_strategy_scheduler_loop(interval: Duration, okx_client: Option<OkxRestClient>) {
    let schedule_enabled = CONFIG.strategy_schedule_enabled();
    info!(
        seconds = interval.as_secs(),
        vol_trigger_enabled = CONFIG.strategy_vol_trigger_enabled(),
        schedule_enabled,
        "strategy scheduler loop enabled"
    );

    strategy_trigger::sync_symbol_states(CONFIG.okx_inst_ids()).await;
    let wake_signal = Arc::new(Notify::new());

    if CONFIG.strategy_vol_trigger_enabled() {
        match okx_client {
            Some(client) => {
                let notify = wake_signal.clone();
                tokio::spawn(async move { run_volatility_trigger_loop(client, notify).await });
            }
            None => warn!("volatility trigger enabled but OKX client is unavailable"),
        }
    }

    loop {
        let now = Instant::now();
        let due_symbols = strategy_trigger::due_symbols(now, schedule_enabled).await;

        if due_symbols.is_empty() {
            if schedule_enabled {
                match strategy_trigger::next_due_instant().await {
                    Some(next_instant) if next_instant > Instant::now() => {
                        let notified = wake_signal.notified();
                        tokio::select! {
                            _ = notified => {},
                            _ = tokio::time::sleep_until(next_instant) => {},
                        }
                    }
                    Some(_) => continue,
                    None => {
                        let notified = wake_signal.notified();
                        tokio::select! {
                            _ = notified => {},
                            _ = tokio::time::sleep(interval) => {},
                        }
                    }
                }
            } else {
                wake_signal.notified().await;
            }
            continue;
        }

        for (symbol, source) in due_symbols {
            let result = run_strategy_analysis_for_symbol(
                &symbol,
                source,
                WS_RETRY_DELAY,
                WS_RETRY_MAX_ATTEMPTS,
            )
            .await;

            let state_snapshot = strategy_trigger::get_symbol_state(&symbol).await;
            let price_delta = state_snapshot
                .as_ref()
                .and_then(|state| strategy_trigger::compute_price_delta(state));
            log_trigger_outcome(&symbol, source, &result, price_delta);

            let last_price = state_snapshot
                .as_ref()
                .and_then(|state| state.last_tick_price);
            match result {
                AnalysisRunResult::Busy => {
                    tokio::time::sleep(interval).await;
                }
                _ => {
                    strategy_trigger::mark_trigger_completion(
                        &symbol, interval, source, last_price,
                    )
                    .await;
                }
            }
        }
    }
}

async fn run_strategy_analysis_for_symbol(
    symbol: &str,
    source: TriggerSource,
    ws_retry_delay: Duration,
    ws_retry_max_attempts: usize,
) -> AnalysisRunResult {
    let mut attempts = 0;
    loop {
        match trigger_analysis(Some(symbol)).await {
            Ok(result) => {
                return AnalysisRunResult::Success {
                    response_symbol: result.symbol,
                    summary_len: result.summary.len(),
                };
            }
            Err(err) if is_analysis_busy_error(&err) => {
                info!(
                    %symbol,
                    ?source,
                    "strategy analysis skipped because previous run is active"
                );
                return AnalysisRunResult::Busy;
            }
            Err(err) if is_websocket_uninitialized_error(&err) => {
                if attempts >= ws_retry_max_attempts {
                    warn!(
                        %symbol,
                        attempts,
                        ?source,
                        "strategy analysis aborted after websocket retries"
                    );
                    return AnalysisRunResult::Failed { error: err };
                }
                attempts += 1;
                warn!(
                    %symbol,
                    attempts,
                    ?source,
                    "strategy analysis deferred: websocket not ready, retrying soon"
                );
                tokio::time::sleep(ws_retry_delay).await;
                continue;
            }
            Err(err) => {
                return AnalysisRunResult::Failed { error: err };
            }
        }
    }
}

async fn run_volatility_trigger_loop(client: OkxRestClient, notify: Arc<Notify>) {
    let symbols = CONFIG.okx_inst_ids().to_vec();
    if symbols.is_empty() {
        warn!("volatility trigger loop started with empty symbol list");
        return;
    }
    info!(
        poll_secs = VOLATILITY_POLL_INTERVAL.as_secs(),
        "volatility trigger loop enabled"
    );
    let threshold_bps = CONFIG.strategy_vol_threshold_bps();
    let mut interval = tokio::time::interval(VOLATILITY_POLL_INTERVAL);
    loop {
        interval.tick().await;
        for symbol in &symbols {
            match client.get_ticker(symbol).await {
                Ok(ticker) => {
                    let Ok(price) = ticker.last.parse::<f64>() else {
                        warn!(value = %ticker.last, %symbol, "failed to parse ticker price");
                        continue;
                    };
                    if let Some(info) = strategy_trigger::record_tick_price(
                        symbol,
                        price,
                        threshold_bps,
                        CONFIG.strategy_vol_window_secs(),
                    )
                    .await
                    {
                        info!(
                            %symbol,
                            price_now = info.price_now,
                            base_price = info.base_price,
                            delta_bps = info.delta_bps,
                            "volatility threshold exceeded; scheduling analysis"
                        );
                        notify.notify_waiters();
                    }
                }
                Err(err) => {
                    warn!(error = ?err, %symbol, "failed to fetch ticker for volatility trigger");
                }
            }
        }
    }
}

fn log_trigger_outcome(
    symbol: &str,
    source: TriggerSource,
    result: &AnalysisRunResult,
    price_snapshot: Option<PriceDeltaSnapshot>,
) {
    match result {
        AnalysisRunResult::Success {
            response_symbol,
            summary_len,
        } => {
            info!(
                %symbol,
                source = ?source,
                summary_len,
                response_symbol = response_symbol.as_deref().unwrap_or(symbol),
                price_now = price_snapshot.as_ref().map(|p| p.price_now),
                base_price = price_snapshot.as_ref().map(|p| p.base_price),
                delta_bps = price_snapshot.as_ref().map(|p| p.delta_bps),
                "strategy analysis completed"
            );
        }
        AnalysisRunResult::Busy => {
            info!(
                %symbol,
                source = ?source,
                price_now = price_snapshot.as_ref().map(|p| p.price_now),
                base_price = price_snapshot.as_ref().map(|p| p.base_price),
                delta_bps = price_snapshot.as_ref().map(|p| p.delta_bps),
                "strategy analysis skipped because previous run is active"
            );
        }
        AnalysisRunResult::Failed { error } => {
            warn!(
                %symbol,
                source = ?source,
                error = %error,
                price_now = price_snapshot.as_ref().map(|p| p.price_now),
                base_price = price_snapshot.as_ref().map(|p| p.base_price),
                delta_bps = price_snapshot.as_ref().map(|p| p.delta_bps),
                "strategy analysis failed"
            );
        }
    }
}

enum AnalysisRunResult {
    Success {
        response_symbol: Option<String>,
        summary_len: usize,
    },
    Busy,
    Failed {
        error: String,
    },
}
