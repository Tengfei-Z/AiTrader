use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{okx, settings::CONFIG, types::ApiResponse, AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub symbol: String,
    pub last: String,
    pub bid_px: Option<String>,
    pub ask_px: Option<String>,
    pub high24h: Option<String>,
    pub low24h: Option<String>,
    pub vol24h: Option<String>,
    pub timestamp: String,
}

impl From<okx::models::Ticker> for Ticker {
    fn from(value: okx::models::Ticker) -> Self {
        Self {
            symbol: value.inst_id,
            last: value.last,
            bid_px: value.bid_px,
            ask_px: value.ask_px,
            high24h: value.high_24h,
            low24h: value.low_24h,
            vol24h: value.vol_24h,
            timestamp: value.ts,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SymbolQuery {
    symbol: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ticker", get(get_ticker))
}

async fn get_ticker(
    State(state): State<AppState>,
    Query(SymbolQuery { symbol }): Query<SymbolQuery>,
) -> impl IntoResponse {
    let use_simulated = CONFIG.okx_use_simulated();
    tracing::info!(symbol = %symbol, use_simulated, "received ticker request");

    if let Some(client) = state.okx_client.clone() {
        match client.get_ticker(&symbol).await {
            Ok(remote) => {
                tracing::info!(
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
