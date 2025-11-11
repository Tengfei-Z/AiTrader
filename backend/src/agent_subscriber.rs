use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{error, info, warn};
use url::Url;
use uuid::Uuid;

use crate::db;
use crate::settings::CONFIG;

/// 全局 WebSocket 发送器（用于其他模块发送消息到 Agent）
static WS_SENDER: once_cell::sync::OnceCell<mpsc::UnboundedSender<OutgoingMessage>> =
    once_cell::sync::OnceCell::new();

/// 待处理的分析请求（用于关联请求和响应）
type PendingAnalyses = Arc<Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<AnalysisResult>>>>;

/// 发送到 Agent 的消息
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutgoingMessage {
    TriggerAnalysis { request_id: String },
}

/// 从 Agent 接收的分析结果
#[derive(Debug, Clone, Deserialize)]
pub struct AnalysisResult {
    pub summary: String,
    #[serde(default)]
    pub suggestions: Vec<String>,
}

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

    // 创建消息发送通道
    let (tx, mut rx) = mpsc::unbounded_channel::<OutgoingMessage>();
    
    // 注册全局发送器
    if let Err(_) = WS_SENDER.set(tx.clone()) {
        warn!("WebSocket sender already initialized");
    }

    // 待处理的分析请求
    let pending_analyses: PendingAnalyses = Arc::new(Mutex::new(std::collections::HashMap::new()));

    warn!("starting agent websocket subscriber for {ws_url}");

    loop {
        match connect_async(ws_url.clone()).await {
            Ok((stream, _)) => {
                info!("connected to agent event websocket");
                let (mut write, mut read) = stream.split();

                loop {
                    tokio::select! {
                        // 接收来自 Agent 的消息
                        message = read.next() => match message {
                            Some(Ok(Message::Text(text))) => {
                                if let Err(err) = handle_agent_message(&text, pending_analyses.clone()).await {
                                    warn!(error = ?err, "failed to process agent websocket message");
                                }
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
                        
                        // 发送消息到 Agent
                        Some(outgoing) = rx.recv() => {
                            let json = match serde_json::to_string(&outgoing) {
                                Ok(json) => json,
                                Err(err) => {
                                    warn!(error = ?err, "failed to serialize outgoing message");
                                    continue;
                                }
                            };
                            info!(message = %json, "sending message to agent");
                            if let Err(err) = write.send(Message::Text(json)).await {
                                warn!(error = ?err, "failed to send message to agent");
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

/// 触发策略分析（供其他模块调用）
pub async fn trigger_analysis() -> Result<AnalysisResult, String> {
    let sender = WS_SENDER.get().ok_or("WebSocket not initialized")?;
    
    let request_id = Uuid::new_v4().to_string();
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    
    // 注册请求等待响应
    // 注意：这里简化处理，实际应该有超时清理机制
    // TODO: 实现更完善的请求-响应关联机制
    
    // 发送触发消息
    sender
        .send(OutgoingMessage::TriggerAnalysis {
            request_id: request_id.clone(),
        })
        .map_err(|_| "failed to send trigger message")?;
    
    info!(request_id = %request_id, "triggered strategy analysis via websocket");
    
    // 等待响应（带超时）
    match tokio::time::timeout(Duration::from_secs(120), response_rx).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(_)) => Err("response channel closed".to_string()),
        Err(_) => Err("analysis timeout".to_string()),
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

async fn handle_agent_message(payload: &str, pending: PendingAnalyses) -> Result<(), serde_json::Error> {
    let message: AgentMessage = serde_json::from_str(payload)?;
    match message {
        AgentMessage::AnalysisResult(result) => process_analysis_result(result, pending).await,
        AgentMessage::OrderUpdate(event) => process_order_update(event).await,
    }
}

async fn process_analysis_result(
    payload: AnalysisResultPayload,
    _pending: PendingAnalyses,
) -> Result<(), serde_json::Error> {
    info!(
        request_id = %payload.request_id,
        summary_len = payload.analysis.summary.len(),
        suggestions = payload.analysis.suggestions.len(),
        "received analysis result from agent"
    );

    // 存储到数据库
    if let Err(err) = db::insert_strategy_message(db::StrategyMessageInsert {
        summary: payload.analysis.summary.clone(),
    })
    .await
    {
        warn!(error = ?err, "failed to persist analysis result");
    }

    // TODO: 通知等待的请求（需要实现请求-响应关联机制）
    
    Ok(())
}

async fn process_order_update(payload: OrderUpdatePayload) -> Result<(), serde_json::Error> {
    info!(
        ord_id = %payload.ord_id.as_ref().unwrap_or(&"unknown".to_string()),
        symbol = %payload.symbol.as_ref().unwrap_or(&"unknown".to_string()),
        status = %payload.status.as_ref().unwrap_or(&"unknown".to_string()),
        "processing order update event"
    );

    if let Some(event) = payload.into_db_event() {
        if let Err(err) = db::upsert_agent_order(event).await {
            warn!(
                ord_id = %payload.ord_id.as_ref().unwrap_or(&"unknown".to_string()),
                error = ?err,
                "failed to persist agent order event"
            );
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AgentMessage {
    AnalysisResult(AnalysisResultPayload),
    OrderUpdate(OrderUpdatePayload),
}

#[derive(Debug, Deserialize)]
struct AnalysisResultPayload {
    request_id: String,
    analysis: AnalysisData,
}

#[derive(Debug, Deserialize)]
struct AnalysisData {
    summary: String,
    #[serde(default)]
    suggestions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OrderUpdatePayload {
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

impl OrderUpdatePayload {
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
