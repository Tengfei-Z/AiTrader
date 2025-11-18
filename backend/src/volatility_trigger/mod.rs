use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::StatusCode;
use tokio::sync::Notify;
use tracing::{debug, info, warn};

use crate::okx::error::OkxError;
use crate::okx::models::Ticker;
use crate::okx::OkxRestClient;
use crate::strategy_trigger;

/// 运行波动触发器所需的全部配置。
#[derive(Clone)]
pub struct VolatilityTriggerConfig {
    pub symbols: Vec<String>,
    pub poll_interval: Duration,
    pub threshold_bps: u64,
    pub window_secs: u64,
    pub max_attempts: usize,
    pub retry_backoff: Duration,
}

/// 启动波动触发循环。
pub fn spawn_volatility_trigger(
    client: OkxRestClient,
    notify: Arc<Notify>,
    config: VolatilityTriggerConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        run_volatility_trigger_loop(client, notify, config).await;
    })
}

async fn run_volatility_trigger_loop(
    client: OkxRestClient,
    notify: Arc<Notify>,
    config: VolatilityTriggerConfig,
) {
    if config.symbols.is_empty() {
        warn!("volatility trigger loop started with empty symbol list");
        return;
    }

    info!(
        poll_secs = config.poll_interval.as_secs(),
        "volatility trigger loop enabled"
    );

    let mut interval = tokio::time::interval(config.poll_interval);
    loop {
        interval.tick().await;
        for symbol in &config.symbols {
            match fetch_ticker_with_retry(
                &client,
                symbol,
                config.max_attempts,
                config.retry_backoff,
            )
            .await
            {
                Ok(ticker) => {
                    let Ok(price) = ticker.last.parse::<f64>() else {
                        warn!(value = %ticker.last, %symbol, "failed to parse ticker price");
                        continue;
                    };
                    if let Some(info) = strategy_trigger::record_tick_price(
                        symbol,
                        price,
                        config.threshold_bps,
                        config.window_secs,
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

async fn fetch_ticker_with_retry(
    client: &OkxRestClient,
    symbol: &str,
    max_attempts: usize,
    retry_delay: Duration,
) -> Result<Ticker> {
    let attempt_limit = max_attempts.max(1);
    let mut attempts = 0;
    loop {
        attempts += 1;
        match client.get_ticker(symbol).await {
            Ok(ticker) => return Ok(ticker),
            Err(err) if attempts < attempt_limit && should_retry_ticker_error(&err) => {
                debug!(
                    error = ?err,
                    %symbol,
                    attempt = attempts,
                    "transient ticker fetch failure; retrying"
                );
                if !retry_delay.is_zero() {
                    tokio::time::sleep(retry_delay).await;
                }
            }
            Err(err) => return Err(err),
        }
    }
}

fn should_retry_ticker_error(err: &anyhow::Error) -> bool {
    let Some(okx_err) = err.downcast_ref::<OkxError>() else {
        return false;
    };

    match okx_err {
        OkxError::HttpClient(inner) => inner.is_connect() || inner.is_timeout(),
        OkxError::HttpStatusWithBody { status, .. } => {
            status.is_server_error() || *status == StatusCode::TOO_MANY_REQUESTS
        }
        _ => false,
    }
}
