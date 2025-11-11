use std::{
    fs,
    sync::{Arc, OnceLock},
};

mod agent_client;
mod agent_subscriber;
mod db;
mod okx;
mod routes;
mod server_config;
mod settings;
mod types;

use crate::agent_client::AgentClient;
use crate::agent_subscriber::run_agent_events_listener;
use crate::db::init_database;
use crate::okx::OkxRestClient;
use crate::routes::{api_routes, run_balance_snapshot_loop};
use crate::server_config::load_app_config;
use crate::settings::CONFIG;
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
    okx_simulated: Option<OkxRestClient>,
    agent: Option<AgentClient>,
    strategy_run_counter: Arc<tokio::sync::RwLock<u64>>,
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

    if let Err(err) = init_database().await {
        warn!(%err, "数据库初始化过程中出现错误");
    }

    let proxy_options = okx::ProxyOptions {
        http: http_proxy,
        https: https_proxy,
    };
    let simulated_flag = CONFIG.okx_use_simulated();
    let okx_simulated = match OkxRestClient::from_config_with_proxy(
        &CONFIG,
        proxy_options.clone(),
        simulated_flag,
    ) {
        Ok(client) => {
            info!(simulated = simulated_flag, "Initialized OKX client");
            Some(client)
        }
        Err(err) => {
            tracing::error!(error = ?err, simulated = simulated_flag, "Failed to initialise OKX client");
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
        strategy_run_counter: Arc::new(tokio::sync::RwLock::new(0)),
    };

    let background_state = app_state.clone();
    tokio::spawn(async move { run_balance_snapshot_loop(background_state).await });
    tokio::spawn(async { run_agent_events_listener().await });

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
        .add_directive("reqwest=debug".parse().unwrap())
        .add_directive("hyper=debug".parse().unwrap());

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
