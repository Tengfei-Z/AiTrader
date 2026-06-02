use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{okx, settings::CONFIG, types::ApiResponse, AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    pub symbol: String,
    pub bar: String,
    pub last: String,
    pub open24h: Option<String>,
    pub bid_px: Option<String>,
    pub ask_px: Option<String>,
    pub high24h: Option<String>,
    pub low24h: Option<String>,
    pub vol24h: Option<String>,
    pub vol_ccy24h: Option<String>,
    pub timestamp: String,
}

impl From<okx::models::Ticker> for Ticker {
    fn from(value: okx::models::Ticker) -> Self {
        Self {
            symbol: value.inst_id,
            bar: value.bar,
            last: value.last,
            open24h: value.open_24h,
            bid_px: value.bid_px,
            ask_px: value.ask_px,
            high24h: value.high_24h,
            low24h: value.low_24h,
            vol24h: value.vol_24h,
            vol_ccy24h: value.vol_ccy_24h,
            timestamp: value.ts,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SymbolQuery {
    symbol: String,
}

#[derive(Debug, Deserialize)]
struct KlineQuery {
    symbol: String,
    bar: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kline {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub volume_ccy: f64,
}

impl From<okx::models::Kline> for Kline {
    fn from(value: okx::models::Kline) -> Self {
        Self {
            timestamp: value.timestamp,
            open: value.open,
            high: value.high,
            low: value.low,
            close: value.close,
            volume: value.volume,
            volume_ccy: value.volume_ccy,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlinePayload {
    pub symbol: String,
    pub bar: String,
    pub candles: Vec<Kline>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ticker", get(get_ticker))
        .route("/klines", get(get_klines))
}

async fn get_ticker(
    State(state): State<AppState>,
    Query(SymbolQuery { symbol }): Query<SymbolQuery>,
) -> impl IntoResponse {
    let use_simulated = CONFIG.okx_use_simulated();
    tracing::trace!(symbol = %symbol, use_simulated, "received ticker request");

    if let Some(client) = state.okx_client.clone() {
        match client.get_ticker(&symbol).await {
            Ok(remote) => {
                tracing::trace!(
                    symbol = %symbol,
                    use_simulated,
                    last = %remote.last,
                    bid = remote.bid_px.as_deref().unwrap_or(""),
                    ask = remote.ask_px.as_deref().unwrap_or(""),
                    "okx ticker hit"
                );
                let mut ticker = Ticker::from(remote);
                ticker.symbol = symbol.clone();
                return Json(ApiResponse::ok(ticker));
            }
            Err(err) => {
                tracing::warn!(symbol = %symbol, error = ?err, use_simulated, "okx ticker fetch failed")
            }
        }
    }

    Json(ApiResponse::<Ticker>::error(format!(
        "symbol {symbol} not found"
    )))
}

async fn get_klines(
    State(state): State<AppState>,
    Query(query): Query<KlineQuery>,
) -> impl IntoResponse {
    let symbol = query.symbol;
    let bar = query.bar.unwrap_or_else(|| "3m".to_string());
    let limit = query.limit.unwrap_or(120).clamp(1, 300);
    let use_simulated = CONFIG.okx_use_simulated();
    tracing::trace!(
        symbol = %symbol,
        bar = %bar,
        limit,
        use_simulated,
        "received klines request"
    );

    if let Some(client) = state.okx_client.clone() {
        match client.get_klines(&symbol, &bar, Some(limit)).await {
            Ok(remote) => {
                let candles = remote.into_iter().map(Kline::from).collect::<Vec<_>>();
                return Json(ApiResponse::ok(KlinePayload {
                    symbol,
                    bar,
                    candles,
                }));
            }
            Err(err) => {
                tracing::warn!(
                    symbol = %symbol,
                    bar = %bar,
                    limit,
                    error = ?err,
                    use_simulated,
                    "okx klines fetch failed"
                );
            }
        }
    }

    Json(ApiResponse::<KlinePayload>::error(format!(
        "klines for {symbol} not found"
    )))
}
