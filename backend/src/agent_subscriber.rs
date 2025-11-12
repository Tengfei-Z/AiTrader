use std::{collections::VecDeque, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{error, info, warn};
use url::Url;

use crate::db;
use crate::order_sync;
use crate::settings::CONFIG;

/// 全局 WebSocket 发送器（用于其他模块发送消息到 Agent）
static WS_SENDER: once_cell::sync::OnceCell<mpsc::UnboundedSender<OutgoingMessage>> =
    once_cell::sync::OnceCell::new();

/// 触发分析的等待队列（无需标识即可顺序匹配）
static PENDING_ANALYSES: once_cell::sync::OnceCell<PendingAnalyses> =
    once_cell::sync::OnceCell::new();

/// 待处理的分析请求队列（用于关联请求和响应）
type PendingAnalyses = Arc<Mutex<VecDeque<oneshot::Sender<AnalysisResult>>>>;

/// 发送到 Agent 的消息
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutgoingMessage {
    TriggerAnalysis,
}

/// 从 Agent 接收的分析结果
#[derive(Debug, Clone, Deserialize)]
pub struct AnalysisResult {
    pub summary: String,
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
    let pending_analyses: PendingAnalyses = Arc::new(Mutex::new(VecDeque::new()));

    if let Err(_) = PENDING_ANALYSES.set(pending_analyses.clone()) {
        warn!("pending analysis queue already initialized");
    }

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
    let pending = PENDING_ANALYSES
        .get()
        .ok_or("analysis queue not initialized")?;

    let (response_tx, response_rx) = oneshot::channel();
    {
        let mut queue = pending.lock().await;
        queue.push_back(response_tx);
    }

    // 发送触发消息
    sender
        .send(OutgoingMessage::TriggerAnalysis)
        .map_err(|_| "failed to send trigger message")?;

    info!("triggered strategy analysis via websocket");

    // 等待响应（带超时）
    match tokio::time::timeout(Duration::from_secs(120), response_rx).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(_)) => Err("response channel closed".to_string()),
        Err(_) => Err("analysis timeout".to_string()),
    }
}

fn build_events_url(base_url: &str) -> Result<Url, url::ParseError> {
    let url = Url::parse(base_url)?;
    if url.scheme() != "ws" {
        return Err(url::ParseError::RelativeUrlWithoutBase);
    }
    Ok(url)
}

async fn handle_agent_message(
    payload: &str,
    pending: PendingAnalyses,
) -> Result<(), serde_json::Error> {
    let message: AgentMessage = serde_json::from_str(payload)?;
    match message {
        AgentMessage::AnalysisResult(result) => process_analysis_result(result, pending).await,
        AgentMessage::OrderUpdate(event) => process_order_update(event).await,
    }
}

async fn process_analysis_result(
    payload: AnalysisResultPayload,
    pending: PendingAnalyses,
) -> Result<(), serde_json::Error> {
    info!(
        summary_len = payload.analysis.summary.len(),
        "received analysis result from agent"
    );

    // 存储到数据库
    let response = AnalysisResult {
        summary: payload.analysis.summary.clone(),
    };

    if let Err(err) = db::insert_strategy_message(db::StrategyMessageInsert {
        summary: response.summary.clone(),
    })
    .await
    {
        warn!(error = ?err, "failed to persist analysis result");
    }

    let mut queue = pending.lock().await;
    if let Some(next) = queue.pop_front() {
        if let Err(err) = next.send(response) {
            warn!(error = ?err, "failed to deliver analysis result to caller");
        }
    } else {
        warn!("received analysis result but no caller is waiting");
    }

    Ok(())
}

async fn process_order_update(payload: OrderUpdatePayload) -> Result<(), serde_json::Error> {
    let ord_id = match payload.ord_id {
        Some(ord_id) => ord_id,
        None => {
            warn!("received agent order update without ord_id");
            return Ok(());
        }
    };

    if let Err(err) = order_sync::process_agent_order_event(&ord_id).await {
        warn!(error = ?err, ord_id = %ord_id, "failed to sync agent order");
    } else {
        info!(ord_id = %ord_id, "processed agent order update");
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
    analysis: AnalysisData,
}

#[derive(Debug, Deserialize)]
struct AnalysisData {
    summary: String,
}

#[derive(Debug, Deserialize)]
struct OrderUpdatePayload {
    #[serde(rename = "ordId")]
    ord_id: Option<String>,
}
