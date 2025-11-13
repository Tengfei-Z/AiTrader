use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::agent_subscriber;
use crate::db::fetch_strategy_messages;
use crate::types::ApiResponse;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyMessage {
    pub id: String,
    pub summary: String,
    pub created_at: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/strategy-chat", get(get_strategy_chat))
        .route("/strategy-run", post(trigger_strategy_run))
}

async fn get_strategy_chat() -> impl IntoResponse {
    match fetch_strategy_messages(50).await {
        Ok(records) => {
            let messages = records
                .into_iter()
                .map(|record| StrategyMessage {
                    id: record.id.to_string(),
                    summary: record.summary,
                    created_at: record.created_at.to_rfc3339(),
                })
                .collect::<Vec<_>>();
            Json(ApiResponse::ok(messages))
        }
        Err(err) => {
            tracing::warn!(error = ?err, "failed to fetch strategy chat from database");
            Json(ApiResponse::<Vec<StrategyMessage>>::error(
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
    tracing::info!("Triggering strategy analysis via WebSocket");

    match agent_subscriber::trigger_analysis().await {
        Ok(response) => {
            tracing::info!(
                summary_preview = %truncate_for_log(&response.summary, 256),
                "Agent analysis completed via WebSocket"
            );

            tracing::info!("Strategy run completed and stored in background task");
        }
        Err(err) if agent_subscriber::is_analysis_busy_error(&err) => {
            tracing::info!("Strategy analysis already running; skipping manual trigger");
        }
        Err(err) => {
            tracing::warn!(%err, "Strategy analysis task failed");
        }
    }
}

fn truncate_for_log(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }

    text.chars().take(max_len).collect::<String>() + "…"
}
