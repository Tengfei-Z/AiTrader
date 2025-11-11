use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{error, info, warn};
use url::Url;

use crate::db;
use crate::settings::CONFIG;

pub async fn run_agent_events_listener() {
    let base_url = match CONFIG.agent_base_url() {
        Some(url) => url,
        None => {
            info!("agent websocket subscriber disabled: AGENT_BASE_URL not configured");
            return;
        }
    };

    let ws_url = match build_events_url(base_url) {
        Ok(url) => url,
        Err(err) => {
            error!(error = ?err, "invalid AGENT_BASE_URL for websocket subscriber");
            return;
        }
    };

    warn!("starting agent websocket subscriber for {ws_url}");

    loop {
        match connect_async(ws_url.clone()).await {
            Ok((stream, _)) => {
                info!("connected to agent event websocket");
                let (mut write, mut read) = stream.split();

                loop {
                    tokio::select! {
                        message = read.next() => match message {
                            Some(Ok(Message::Text(text))) => {
                                if let Err(err) = handle_agent_message(&text).await {
                                    warn!(error = ?err, "failed to process agent websocket message");
                                }
                                let _ = write
                                    .send(Message::Text("{\"status\":\"received\"}".to_string()))
                                    .await;
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                let _ = write.send(Message::Pong(payload)).await;
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                info!("agent websocket closed, reconnecting");
                                break;
                            }
                            Some(Ok(_)) => {}
                            Some(Err(err)) => {
                                warn!(error = ?err, "agent websocket read failure");
                                break;
                            }
                        },
                        _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                    }
                }
            }
            Err(err) => {
                warn!(error = ?err, "failed to connect to agent websocket");
            }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
        info!("reconnecting to agent websocket");
    }
}

fn build_events_url(base_url: &str) -> Result<Url, url::ParseError> {
    let mut url = Url::parse(base_url)?;
    let scheme = match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        other => other,
    }
    .to_owned();
    url.set_scheme(&scheme).ok();
    url.set_path("/agent/events/ws");
    Ok(url)
}

async fn handle_agent_message(payload: &str) -> Result<(), serde_json::Error> {
    let message: AgentMessage = serde_json::from_str(payload)?;
    match message {
        AgentMessage::TaskResult(event) => process_task_result(event).await,
    }
}

async fn process_task_result(payload: TaskResultPayload) -> Result<(), serde_json::Error> {
    let analysis = payload.analysis;
    let summary_preview: String = analysis.summary.chars().take(120).collect();
    info!(
        task_id = %payload.task_id,
        status = %payload.status,
        session_id = %analysis.session_id,
        instrument = %analysis.instrument_id,
        summary_preview = %summary_preview,
        suggestions = analysis.suggestions.len(),
        "processing agent task_result event"
    );

    for order_payload in payload.orders {
        let ord_id = order_payload.ord_id.clone().unwrap_or_default();
        if let Some(event) = order_payload.into_db_event() {
            if let Err(err) = db::upsert_agent_order(event).await {
                warn!(
                    task_id = %payload.task_id,
                    ord_id = %ord_id,
                    error = ?err,
                    "failed to persist agent order event"
                );
            }
        }
    }

    Ok(())
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

#[allow(dead_code)]
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
