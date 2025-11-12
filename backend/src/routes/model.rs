use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::agent_subscriber;
use crate::db::{fetch_strategy_messages, insert_strategy_message, StrategyMessageInsert};
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
        if let Err(err) = run_strategy_job().await {
            tracing::warn!(%err, "Strategy analysis task failed");
        }
    });

    Json(ApiResponse::ok(()))
}

async fn run_strategy_job() -> Result<(), String> {
    tracing::info!("Triggering strategy analysis via WebSocket");

    let response = agent_subscriber::trigger_analysis().await?;

    tracing::info!(
        summary_preview = %truncate_for_log(&response.summary, 256),
        "Agent analysis completed via WebSocket"
    );

    let content = format!("【市场分析】\n{}\n", response.summary);

    tracing::debug!("Persisting strategy message to database");

    if let Err(err) = insert_strategy_message(StrategyMessageInsert { summary: content }).await {
        tracing::warn!(%err, "写入策略摘要到数据库失败");
    }

    tracing::info!("Strategy run completed and stored in background task");
    Ok(())
}

fn truncate_for_log(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }

    text.chars().take(max_len).collect::<String>() + "…"
}
