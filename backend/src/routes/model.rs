use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};

use crate::agent_subscriber;
use crate::db::fetch_strategy_messages;
use crate::settings::CONFIG;
use crate::strategy_trigger::{self, TriggerSource};
use crate::types::ApiResponse;
use crate::AppState;

const WS_RETRY_DELAY: Duration = Duration::from_secs(5);
const WS_RETRY_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyMessage {
    pub id: String,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyChatPayload {
    pub allow_manual_trigger: bool,
    pub messages: Vec<StrategyMessage>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/strategy-chat", get(get_strategy_chat))
        .route("/strategy-run", post(trigger_strategy_run))
}

async fn get_strategy_chat() -> impl IntoResponse {
    match fetch_strategy_messages(15).await {
        Ok(records) => {
            let messages = records
                .into_iter()
                .map(|record| StrategyMessage {
                    id: record.id.to_string(),
                    summary: record.summary,
                    created_at: record.created_at.to_rfc3339(),
                })
                .collect::<Vec<_>>();
            let payload = StrategyChatPayload {
                allow_manual_trigger: CONFIG.strategy_manual_trigger_enabled(),
                messages,
            };
            Json(ApiResponse::ok(payload))
        }
        Err(err) => {
            tracing::warn!(error = ?err, "failed to fetch strategy chat from database");
            Json(ApiResponse::<StrategyChatPayload>::error(
                "无法获取策略对话",
            ))
        }
    }
}

async fn trigger_strategy_run() -> impl IntoResponse {
    tracing::info!("HTTP POST /model/strategy-run invoked from UI");

    tracing::info!("Triggering agent strategy analysis via WebSocket");

    tokio::spawn(async move {
        run_strategy_job().await;
    });

    Json(ApiResponse::ok(()))
}

async fn run_strategy_job() {
    let inst_ids: Vec<String> = CONFIG.okx_inst_ids().to_vec();
    strategy_trigger::sync_symbol_states(&inst_ids).await;
    let interval = Duration::from_secs(CONFIG.strategy_schedule_interval_secs());
    for symbol in inst_ids {
        tracing::info!(%symbol, "Triggering strategy analysis via WebSocket");

        let outcome = run_manual_analysis(&symbol).await;
        let state_snapshot = strategy_trigger::get_symbol_state(&symbol).await;
        let price_delta = state_snapshot
            .as_ref()
            .and_then(|state| strategy_trigger::compute_price_delta(state));
        log_manual_trigger_outcome(&symbol, &outcome, price_delta);

        if matches!(outcome, ManualTriggerOutcome::Busy) {
            continue;
        }

        let last_price = state_snapshot
            .as_ref()
            .and_then(|state| state.last_tick_price);
        strategy_trigger::mark_trigger_completion(
            &symbol,
            interval,
            TriggerSource::Manual,
            last_price,
        )
        .await;
    }
}

fn truncate_for_log(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }

    text.chars().take(max_len).collect::<String>() + "…"
}

enum ManualTriggerOutcome {
    Success {
        response_symbol: Option<String>,
        summary: String,
    },
    Busy,
    Failed {
        error: String,
    },
}

async fn run_manual_analysis(symbol: &str) -> ManualTriggerOutcome {
    let mut attempts = 0;
    loop {
        match agent_subscriber::trigger_analysis(Some(symbol)).await {
            Ok(response) => {
                return ManualTriggerOutcome::Success {
                    response_symbol: response.symbol,
                    summary: response.summary,
                };
            }
            Err(err) if agent_subscriber::is_analysis_busy_error(&err) => {
                return ManualTriggerOutcome::Busy;
            }
            Err(err) if agent_subscriber::is_websocket_uninitialized_error(&err) => {
                if attempts >= WS_RETRY_MAX_ATTEMPTS {
                    tracing::warn!(
                        %symbol,
                        attempts,
                        "Strategy analysis aborted after websocket initialization retries"
                    );
                    return ManualTriggerOutcome::Failed { error: err };
                }
                attempts += 1;
                tracing::warn!(
                    %symbol,
                    attempts,
                    "Strategy analysis deferred: websocket not ready, retrying shortly"
                );
                sleep(WS_RETRY_DELAY).await;
                continue;
            }
            Err(err) => {
                return ManualTriggerOutcome::Failed { error: err };
            }
        }
    }
}

fn log_manual_trigger_outcome(
    symbol: &str,
    outcome: &ManualTriggerOutcome,
    price_delta: Option<strategy_trigger::PriceDeltaSnapshot>,
) {
    match outcome {
        ManualTriggerOutcome::Success {
            response_symbol,
            summary,
        } => {
            tracing::info!(
                %symbol,
                source = ?TriggerSource::Manual,
                response_symbol = response_symbol.as_deref().unwrap_or(symbol),
                summary_preview = %truncate_for_log(summary, 256),
                price_now = price_delta.as_ref().map(|p| p.price_now),
                base_price = price_delta.as_ref().map(|p| p.base_price),
                delta_bps = price_delta.as_ref().map(|p| p.delta_bps),
                "Manual strategy analysis completed via WebSocket"
            );
        }
        ManualTriggerOutcome::Busy => {
            tracing::info!(
                %symbol,
                source = ?TriggerSource::Manual,
                price_now = price_delta.as_ref().map(|p| p.price_now),
                base_price = price_delta.as_ref().map(|p| p.base_price),
                delta_bps = price_delta.as_ref().map(|p| p.delta_bps),
                "Strategy analysis already running; skipping manual trigger"
            );
        }
        ManualTriggerOutcome::Failed { error } => {
            tracing::warn!(
                %symbol,
                source = ?TriggerSource::Manual,
                error = %error,
                price_now = price_delta.as_ref().map(|p| p.price_now),
                base_price = price_delta.as_ref().map(|p| p.base_price),
                delta_bps = price_delta.as_ref().map(|p| p.delta_bps),
                "Strategy analysis task failed"
            );
        }
    }
}
