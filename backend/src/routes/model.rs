use std::time::Duration;

use anyhow::{anyhow, Result};
use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::agent_client::AgentClient;
use crate::db::{fetch_strategy_messages, insert_strategy_message, StrategyMessageInsert};
use crate::types::ApiResponse;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

async fn trigger_strategy_run(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("HTTP POST /model/strategy-run invoked from UI");

    let Some(agent_client) = state.agent.clone() else {
        tracing::error!("Agent client not initialised");
        return Json(ApiResponse::<()>::error("AI Agent 未配置或初始化失败"));
    };

    tracing::info!("Triggering agent strategy analysis");

    tokio::spawn(async move {
        if let Err(err) = run_strategy_job(agent_client).await {
            tracing::warn!(%err, "Strategy analysis task failed");
        }
    });

    Json(ApiResponse::ok(()))
}

async fn run_strategy_job(agent_client: AgentClient) -> Result<()> {
    let timeout_budget = Duration::from_secs(60);

    let response = match tokio::time::timeout(timeout_budget, agent_client.analysis()).await
    {
        Err(_) => {
            tracing::error!("Agent analysis timed out");
            return Err(anyhow!("agent_analysis_timeout"));
        }
        Ok(result) => match result {
            Ok(resp) => resp,
            Err(err) => {
                tracing::error!(error = %err, "Agent analysis failed");
                return Err(err);
            }
        },
    };

    tracing::info!(
        completed_at = %response.created_at,
        summary_preview = %truncate_for_log(&response.summary, 256),
        suggestions = response.suggestions.len(),
        "Agent analysis completed"
    );

    let mut content = format!("【市场分析】\n{}\n", response.summary);
    if !response.suggestions.is_empty() {
        content.push_str("\n【策略建议】\n");
        for suggestion in &response.suggestions {
            content.push_str("- ");
            content.push_str(suggestion);
            content.push('\n');
        }
    }

    tracing::debug!("Persisting strategy message to database");

    if let Err(err) = insert_strategy_message(StrategyMessageInsert {
        summary: content,
    })
    .await
    {
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
