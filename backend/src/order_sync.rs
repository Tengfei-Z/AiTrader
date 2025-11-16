use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::{DateTime, TimeZone, Utc};
use once_cell::sync::OnceCell;
use serde_json;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::db::{self, PositionSnapshot, TradeRecord};
use crate::okx::{self, OkxRestClient};
use crate::settings::CONFIG;

static ORDER_SYNC_CLIENT: OnceCell<Option<OkxRestClient>> = OnceCell::new();

const DEFAULT_INST_TYPE: &str = "SWAP";

/// 初始化 order-sync 所需的 OKX 客户端实例。
pub fn init_client(client: Option<OkxRestClient>) {
    if ORDER_SYNC_CLIENT.set(client).is_err() {
        warn!("order sync client already initialized");
    }
}

fn okx_client() -> Option<OkxRestClient> {
    ORDER_SYNC_CLIENT
        .get()
        .and_then(|client_opt| client_opt.clone())
}

/// 处理 agent 推送的 ordId 事件，把数据写入 `orders`/`trades`/`positions`。
const ORDER_HISTORY_PAGE_LIMIT: i64 = 100;

pub async fn process_agent_order_event(ord_id: &str) -> Result<()> {
    let client = okx_client().ok_or_else(|| anyhow!("order sync okx client unavailable"))?;

    let mut order = None;
    let mut fills_cache: Option<Vec<okx::models::FillDetail>> = None;
    for inst_id in CONFIG.okx_inst_ids().iter() {
        let historical_orders = client
            .get_order_history(
                Some(DEFAULT_INST_TYPE),
                Some(inst_id.as_str()),
                None,
                Some(ord_id),
                Some(ORDER_HISTORY_PAGE_LIMIT),
            )
            .await?;
        debug!(
            inst_id = %inst_id,
            ord_id = %ord_id,
            fetched = historical_orders.len(),
            "scanned order history page"
        );

        if let Some(entry) = historical_orders
            .into_iter()
            .find(|entry| entry.ord_id == ord_id)
        {
            order = Some(entry);
            break;
        }
    }

    if order.is_none() {
        warn!(
            ord_id = %ord_id,
            "order not found in initial history scan, attempting fill-based lookup"
        );
        match client.get_fills(None, Some(ord_id), Some(50)).await {
            Ok(fallback_fills) => {
                if let Some(first_fill) = fallback_fills.first() {
                    debug!(
                        ord_id = %ord_id,
                        inst_id = %first_fill.inst_id,
                        "using fill inst_id for history lookup"
                    );
                    let historical_orders = client
                        .get_order_history(
                            Some(DEFAULT_INST_TYPE),
                            Some(first_fill.inst_id.as_str()),
                            None,
                            Some(ord_id),
                            Some(ORDER_HISTORY_PAGE_LIMIT),
                        )
                        .await?;
                    if let Some(entry) = historical_orders
                        .into_iter()
                        .find(|entry| entry.ord_id == ord_id)
                    {
                        order = Some(entry);
                        fills_cache = Some(fallback_fills);
                    }
                }
            }
            Err(err) => warn!(
                error = ?err,
                ord_id = %ord_id,
                "failed to fetch fallback fills for lookup"
            ),
        }
    }

    let order = order.ok_or_else(|| anyhow!("order history missing {ord_id}"))?;

    info!(?order, "fetched order history entry");

    let event = event_from_order_detail(&order)?;
    db::upsert_agent_order(event).await?;

    let fills = if let Some(cached) = fills_cache {
        cached
    } else {
        client
            .get_fills(Some(order.inst_id.as_str()), Some(ord_id), Some(50))
            .await?
    };

    for fill in fills {
        let record = trade_record_from_fill(ord_id, &order, &fill)?;
        db::insert_trade_record(record).await?;
    }

    sync_positions_from_okx(&client).await?;

    Ok(())
}

/// 周期性同步 OKX 最新持仓/成交状态，防止 agent 只推一次 ordId 时状态无法补全。
pub async fn run_periodic_position_sync() {
    loop {
        if let Some(client) = okx_client() {
            if let Err(err) = sync_positions_from_okx(&client).await {
                warn!(error = ?err, "periodic position sync failed");
            }
        } else {
            warn!("skipping periodic position sync: okx client unavailable");
        }

        sleep(Duration::from_secs(60)).await;
    }
}

async fn sync_positions_from_okx(client: &OkxRestClient) -> Result<()> {
    let positions = client.get_positions(Some(DEFAULT_INST_TYPE)).await?;
    let allowed_inst_ids: HashSet<String> = CONFIG.okx_inst_ids().iter().cloned().collect();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for detail in positions
        .into_iter()
        .filter(|detail| allowed_inst_ids.contains(&detail.inst_id))
    {
        let snapshot = position_snapshot_from_detail(&detail)?;
        let inst_id = snapshot.inst_id.clone();
        let pos_side = snapshot.pos_side.clone();
        seen.insert((inst_id.clone(), pos_side.clone()));

        db::upsert_position_snapshot(snapshot).await?;
    }

    let existing = db::fetch_position_snapshots(false, None, None).await?;
    for snapshot in existing {
        if !allowed_inst_ids.contains(&snapshot.inst_id) {
            continue;
        }
        let key = (snapshot.inst_id.clone(), snapshot.pos_side.clone());
        if !seen.contains(&key) {
            if let Err(err) =
                db::mark_position_forced_exit(&snapshot.inst_id, &snapshot.pos_side).await
            {
                warn!(error = ?err, inst_id = %snapshot.inst_id, pos_side = %snapshot.pos_side, "failed to close missing position");
            }
        }
    }
    Ok(())
}

fn event_from_order_detail(order: &okx::models::OrderHistoryEntry) -> Result<db::AgentOrderEvent> {
    let size = parse_number(Some(&order.sz)).unwrap_or(0.0);
    let filled = parse_number(Some(&order.acc_fill_sz)).unwrap_or(0.0);
    let price =
        parse_number(order.px.as_deref()).or_else(|| parse_number(order.fill_px.as_deref()));
    let leverage = order
        .lever
        .as_deref()
        .and_then(|text| text.parse::<f64>().ok());

    let metadata = serde_json::to_value(order)?;

    Ok(db::AgentOrderEvent {
        ord_id: order.ord_id.clone(),
        inst_id: order.inst_id.clone(),
        side: order.side.clone(),
        order_type: Some(order.ord_type.clone()),
        price,
        size,
        filled_size: Some(filled),
        status: order.state.clone(),
        td_mode: order.td_mode.clone(),
        pos_side: order.pos_side.clone(),
        leverage,
        action_kind: determine_action_kind(order),
        metadata,
    })
}

fn determine_action_kind(order: &okx::models::OrderHistoryEntry) -> Option<String> {
    if order.reduce_only.unwrap_or(false)
        || order
            .tag
            .as_ref()
            .map(|tag| tag.eq_ignore_ascii_case("reduceOnly"))
            .unwrap_or(false)
    {
        return Some("exit".to_string());
    }
    None
}

fn trade_record_from_fill(
    ord_id: &str,
    order: &okx::models::OrderHistoryEntry,
    fill: &okx::models::FillDetail,
) -> Result<TradeRecord> {
    let filled_size = parse_number(fill.fill_sz.as_deref()).unwrap_or(0.0);
    let fill_price = parse_number(fill.fill_px.as_deref());
    let fee = parse_number(fill.fee.as_deref());
    let realized_pnl = parse_number(fill.fill_pnl.as_deref());
    let ts = parse_timestamp_ms(fill.ts.as_deref()).unwrap_or_else(|| Utc::now());

    let fingerprint = generate_fill_fingerprint(order, fill);
    let trade_id = fill.trade_id.clone().or_else(|| Some(fingerprint.clone()));

    Ok(TradeRecord {
        ord_id: ord_id.to_string(),
        trade_id,
        fingerprint: Some(fingerprint),
        inst_id: order.inst_id.clone(),
        td_mode: order.td_mode.clone(),
        pos_side: order.pos_side.clone(),
        side: fill
            .side
            .clone()
            .or_else(|| Some(order.side.clone()))
            .unwrap_or_else(|| "buy".to_string()),
        filled_size,
        fill_price,
        fee,
        realized_pnl,
        ts,
        metadata: serde_json::to_value(fill)?,
    })
}

fn generate_fill_fingerprint(
    order: &okx::models::OrderHistoryEntry,
    fill: &okx::models::FillDetail,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(order.ord_id.as_bytes());
    if let Some(trade_id) = fill.trade_id.as_ref() {
        hasher.update(trade_id.as_bytes());
    }
    if let Some(ts) = fill.ts.as_ref() {
        hasher.update(ts.as_bytes());
    }
    if let Some(sz) = fill.fill_sz.as_ref() {
        hasher.update(sz.as_bytes());
    }
    if let Some(px) = fill.fill_px.as_ref() {
        hasher.update(px.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn position_snapshot_from_detail(detail: &okx::models::PositionDetail) -> Result<PositionSnapshot> {
    let size = parse_number(detail.pos.as_deref()).unwrap_or(0.0);
    let avg_price = parse_number(detail.avg_px.as_deref());
    let mark_px = parse_number(detail.mark_px.as_deref());
    let margin = parse_number(detail.margin.as_deref());
    let unrealized_pnl = parse_number(detail.upl.as_deref());
    let last_trade_at = parse_timestamp_ms(detail.c_time.as_deref());
    let pos_side = detail
        .pos_side
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "net".to_string());

    Ok(PositionSnapshot {
        inst_id: detail.inst_id.clone(),
        pos_side,
        td_mode: detail.inst_type.clone(),
        side: detail.pos_side.clone().unwrap_or_else(|| "net".to_string()),
        size,
        avg_price,
        mark_px,
        margin,
        unrealized_pnl,
        last_trade_at,
        closed_at: if size == 0.0 { Some(Utc::now()) } else { None },
        action_kind: None,
        entry_ord_id: None,
        exit_ord_id: None,
        metadata: serde_json::to_value(detail)?,
    })
}

fn parse_number(value: Option<&str>) -> Option<f64> {
    value
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|num| num.is_finite())
}

fn parse_timestamp_ms(value: Option<&str>) -> Option<DateTime<Utc>> {
    let text = value?.trim();
    if let Ok(ms) = text.parse::<i64>() {
        return Utc
            .timestamp_millis_opt(ms)
            .single()
            .or_else(|| Some(Utc::now()));
    }
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}
