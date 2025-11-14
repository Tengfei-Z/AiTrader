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
    for symbol in inst_ids {
        tracing::info!(%symbol, "Triggering strategy analysis via WebSocket");

        let mut attempts = 0;
        loop {
            match agent_subscriber::trigger_analysis(Some(symbol.as_str())).await {
                Ok(response) => {
                    tracing::info!(
                        summary_preview = %truncate_for_log(&response.summary, 256),
                        symbol = response.symbol.as_deref().unwrap_or(&symbol),
                        "Agent analysis completed via WebSocket"
                    );
                    tracing::info!("Strategy run completed and stored in background task");
                    break;
                }
                Err(err) if agent_subscriber::is_analysis_busy_error(&err) => {
                    tracing::info!(%symbol, "Strategy analysis already running; skipping manual trigger");
                    break;
                }
                Err(err) if agent_subscriber::is_websocket_uninitialized_error(&err) => {
                    if attempts >= WS_RETRY_MAX_ATTEMPTS {
                        tracing::warn!(
                            %symbol,
                            attempts,
                            "Strategy analysis aborted after websocket initialization retries"
                        );
                        break;
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
                    tracing::warn!(%err, %symbol, "Strategy analysis task failed");
                    break;
                }
            }
        }
    }
}

fn truncate_for_log(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }

    text.chars().take(max_len).collect::<String>() + "…"
}
