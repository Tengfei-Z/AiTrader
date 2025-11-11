use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::db;

pub async fn agent_websocket(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_agent_socket)
}

async fn handle_agent_socket(mut socket: WebSocket) {
    while let Some(result) = socket.next().await {
        match result {
            Ok(Message::Text(payload)) => match serde_json::from_str::<AgentMessage>(&payload) {
                Ok(message) => {
                    info!("received agent message");
                    handle_agent_message(message).await;
                    let _ = socket
                        .send(Message::Text(json!({"status": "ok"}).to_string()))
                        .await;
                }
                Err(err) => {
                    warn!(error = ?err, "failed to parse agent websocket message");
                    let _ = socket
                        .send(Message::Text(
                            json!({"status": "error", "reason": err.to_string()}).to_string(),
                        ))
                        .await;
                }
            },
            Ok(Message::Ping(payload)) => {
                let _ = socket.send(Message::Pong(payload)).await;
            }
            Ok(Message::Close(frame)) => {
                let _ = socket.send(Message::Close(frame)).await;
                break;
            }
            Ok(_) => {}
            Err(err) => {
                warn!(error = ?err, "agent websocket connection failed");
                break;
            }
        }
    }
}

async fn handle_agent_message(message: AgentMessage) {
    match message {
        AgentMessage::TaskResult(payload) => {
            let analysis = &payload.analysis;
            let summary_preview: String = analysis.summary.chars().take(120).collect();
            info!(
                task_id = %payload.task_id,
                status = %payload.status,
                session_id = %analysis.session_id,
                instrument = %analysis.instrument_id,
                summary_preview = %summary_preview,
                suggestions = analysis.suggestions.len(),
                "processing task_result event"
            );
            for order_payload in payload.orders {
                let ord_id = order_payload.ord_id.clone().unwrap_or_default();
                if let Some(order_event) = order_payload.into_db_event() {
                    if let Err(err) = db::upsert_agent_order(order_event).await {
                        warn!(
                            task_id = %payload.task_id,
                            ord_id = %ord_id,
                            error = ?err,
                            "failed to persist agent order event"
                        );
                    }
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AgentMessage {
    TaskResult(TaskResultPayload),
}

#[derive(Debug, Deserialize)]
struct TaskResultPayload {
    task_id: String,
    status: String,
    analysis: AgentAnalysisPayload,
    orders: Vec<AgentOrderPayload>,
}

#[derive(Debug, Deserialize)]
struct AgentAnalysisPayload {
    session_id: String,
    instrument_id: String,
    analysis_type: String,
    summary: String,
    suggestions: Vec<String>,
    completed_at: String,
}

#[derive(Debug, Deserialize)]
struct AgentOrderPayload {
    #[serde(rename = "ordId")]
    ord_id: Option<String>,
    symbol: Option<String>,
    side: Option<String>,
    #[serde(rename = "order_type")]
    order_type: Option<String>,
    price: Option<Value>,
    size: Option<Value>,
    #[serde(rename = "filled_size")]
    filled_size: Option<Value>,
    status: Option<String>,
    #[serde(default)]
    metadata: Value,
}

impl AgentOrderPayload {
    fn into_db_event(self) -> Option<db::AgentOrderEvent> {
        let ord_id = self.ord_id?;
        Some(db::AgentOrderEvent {
            ord_id: ord_id.clone(),
            symbol: self.symbol.unwrap_or_else(|| "unknown".to_string()),
            side: self
                .side
                .map(|value| value.to_lowercase())
                .filter(|value| matches!(value.as_str(), "buy" | "sell"))
                .unwrap_or_else(|| "buy".to_string()),
            order_type: self.order_type,
            price: parse_value_to_f64(&self.price),
            size: parse_value_to_f64(&self.size).unwrap_or(0.0),
            filled_size: parse_value_to_f64(&self.filled_size),
            status: self.status.unwrap_or_else(|| "open".to_string()),
            metadata: self.metadata,
        })
    }
}

fn parse_value_to_f64(value: &Option<Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => text.parse::<f64>().ok(),
        _ => None,
    }
}
