use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, Mutex, Semaphore};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{error, info, warn};
use url::Url;

use crate::db;
use crate::order_sync;
use crate::settings::CONFIG;

/// 全局 WebSocket 发送器（用于其他模块发送消息到 Agent）
static WS_SENDER: OnceCell<mpsc::UnboundedSender<OutgoingMessage>> = OnceCell::new();

/// 触发分析的等待队列（无需标识即可顺序匹配）
static PENDING_ANALYSES: OnceCell<PendingAnalyses> = OnceCell::new();

/// 控制策略分析串行执行，避免重复触发。
static ANALYSIS_PERMIT: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(1)));

const ANALYSIS_BUSY_ERROR: &str = "analysis already running";
static NEXT_PENDING_ID: AtomicU64 = AtomicU64::new(1);

/// 待处理的分析请求（携带标识便于在超时/断连时清理）
#[derive(Debug)]
struct PendingAnalysis {
    id: u64,
    sender: oneshot::Sender<Result<AnalysisResult, String>>,
}

/// 待处理的分析请求队列（用于关联请求和响应）
type PendingAnalyses = Arc<Mutex<VecDeque<PendingAnalysis>>>;

/// 发送到 Agent 的消息
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutgoingMessage {
    TriggerAnalysis { symbol: Option<String> },
}

/// 从 Agent 接收的分析结果
#[derive(Debug, Clone, Deserialize)]
pub struct AnalysisResult {
    pub summary: String,
    pub symbol: Option<String>,
}

pub async fn run_agent_events_listener() {
    let base_url = match CONFIG.agent_base_url() {
        Some(url) => url,
        None => {
            warn!("agent websocket subscriber disabled: AGENT_BASE_URL not configured");
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

    info!("starting agent websocket subscriber for {ws_url}");

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
                fail_all_pending(pending_analyses.clone(), "agent websocket disconnected").await;
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
pub async fn trigger_analysis(symbol: Option<&str>) -> Result<AnalysisResult, String> {
    let semaphore = Lazy::force(&ANALYSIS_PERMIT).clone();
    let permit = match semaphore.try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            info!("strategy analysis already in progress, skipping trigger");
            return Err(ANALYSIS_BUSY_ERROR.to_string());
        }
    };

    let result = trigger_analysis_inner(symbol.map(|s| s.to_string())).await;
    drop(permit);

    result
}

async fn trigger_analysis_inner(symbol: Option<String>) -> Result<AnalysisResult, String> {
    let sender = WS_SENDER.get().ok_or("WebSocket not initialized")?;
    let pending = PENDING_ANALYSES
        .get()
        .ok_or("analysis queue not initialized")?
        .clone();

    let (response_tx, response_rx) = oneshot::channel();
    let request_id = NEXT_PENDING_ID.fetch_add(1, Ordering::Relaxed);
    {
        let mut queue = pending.lock().await;
        queue.push_back(PendingAnalysis {
            id: request_id,
            sender: response_tx,
        });
    }

    // 发送触发消息
    if let Err(_) = sender.send(OutgoingMessage::TriggerAnalysis { symbol }) {
        remove_pending_by_id(pending.clone(), request_id).await;
        return Err("failed to send trigger message".to_string());
    }

    info!("triggered strategy analysis via websocket");

    // 等待响应（带超时）
    match tokio::time::timeout(Duration::from_secs(120), response_rx).await {
        Ok(Ok(Ok(result))) => Ok(result),
        Ok(Ok(Err(err))) => Err(err),
        Ok(Err(_)) => {
            remove_pending_by_id(pending.clone(), request_id).await;
            Err("response channel closed".to_string())
        }
        Err(_) => {
            remove_pending_by_id(pending.clone(), request_id).await;
            Err("analysis timeout".to_string())
        }
    }
}

pub fn is_analysis_busy_error(err: &str) -> bool {
    err == ANALYSIS_BUSY_ERROR
}

pub fn is_websocket_uninitialized_error(err: &str) -> bool {
    err.eq_ignore_ascii_case("websocket not initialized")
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
        AgentMessage::AnalysisError(error) => process_analysis_error(error, pending).await,
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
        symbol: payload.analysis.symbol.clone(),
    };

    if let Err(err) = db::insert_strategy_message(db::StrategyMessageInsert {
        summary: response.summary.clone(),
    })
    .await
    {
        warn!(error = ?err, "failed to persist analysis result");
    }

    let mut queue = pending.lock().await;
    if let Some(entry) = queue.pop_front() {
        if let Err(err) = entry.sender.send(Ok(response)) {
            warn!(
                error = ?err,
                request_id = entry.id,
                "failed to deliver analysis result to caller"
            );
        }
    } else {
        warn!("received analysis result but no caller is waiting");
    }

    Ok(())
}

async fn process_analysis_error(
    payload: AnalysisErrorPayload,
    pending: PendingAnalyses,
) -> Result<(), serde_json::Error> {
    warn!(error = %payload.error, "received analysis error from agent");

    let mut queue = pending.lock().await;
    if let Some(entry) = queue.pop_front() {
        if let Err(err) = entry.sender.send(Err(payload.error.clone())) {
            warn!(
                error = ?err,
                request_id = entry.id,
                "failed to deliver analysis error to caller"
            );
        }
    } else {
        warn!("received analysis error but no caller is waiting");
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

    let ord_id_clone = ord_id.clone();
    tokio::spawn(async move {
        if let Err(err) = order_sync::process_agent_order_event(&ord_id_clone).await {
            warn!(error = ?err, ord_id = %ord_id_clone, "failed to sync agent order");
        } else {
            info!(ord_id = %ord_id_clone, "processed agent order update");
        }
    });

    Ok(())
}

async fn remove_pending_by_id(pending: PendingAnalyses, request_id: u64) {
    let mut queue = pending.lock().await;
    let before = queue.len();
    queue.retain(|entry| entry.id != request_id);
    let removed = before.saturating_sub(queue.len());
    if removed > 0 {
        info!(
            request_id = request_id,
            removed = removed,
            "dropped pending analysis sender due to failure"
        );
    }
}

async fn fail_all_pending(pending: PendingAnalyses, reason: &str) {
    let mut queue = pending.lock().await;
    let dropped = queue.len();
    if dropped > 0 {
        queue.clear();
        warn!(
            dropped = dropped,
            reason = %reason,
            "cleared pending strategy analyses"
        );
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AgentMessage {
    AnalysisResult(AnalysisResultPayload),
    AnalysisError(AnalysisErrorPayload),
    OrderUpdate(OrderUpdatePayload),
}

#[derive(Debug, Deserialize)]
struct AnalysisResultPayload {
    analysis: AnalysisData,
}

#[derive(Debug, Deserialize)]
struct AnalysisData {
    summary: String,
    symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnalysisErrorPayload {
    error: String,
}

#[derive(Debug, Deserialize)]
struct OrderUpdatePayload {
    #[serde(rename = "ordId")]
    ord_id: Option<String>,
}
