use std::collections::{BTreeMap, HashSet};

use anyhow::{anyhow, ensure, Context, Result};
use chrono::Utc;
use okx::{models::OrderBookSnapshot as OkxOrderBookSnapshot, OkxRestClient};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use tracing::warn;

const DEFAULT_TIMEFRAME: &str = "3m";
const DEFAULT_QUOTE: &str = "USDT";
const DEFAULT_CANDLE_LIMIT: usize = 200;

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct MarketDataRequest {
    pub coins: Vec<String>,
    pub timeframe: String,
    pub quote: String,
    pub indicators: Vec<String>,
    pub include_orderbook: bool,
    pub include_funding: bool,
    pub include_open_interest: bool,
    pub simulated_trading: bool,
}

impl Default for MarketDataRequest {
    fn default() -> Self {
        Self {
            coins: Vec::new(),
            timeframe: DEFAULT_TIMEFRAME.to_string(),
            quote: DEFAULT_QUOTE.to_string(),
            indicators: vec!["price".to_string()],
            include_orderbook: false,
            include_funding: false,
            include_open_interest: false,
            simulated_trading: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(default)]
pub struct MarketDataResponse {
    pub timestamp: String,
    pub coins: BTreeMap<String, CoinMarketData>,
}

impl Default for MarketDataResponse {
    fn default() -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            coins: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(default)]
pub struct CoinMarketData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_ema20: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_ema50: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_macd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_rsi: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_interest: Option<OpenInterestSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funding_rate: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub price_series: Vec<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ema20_series: Vec<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ema50_series: Vec<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub macd_series: Vec<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rsi7_series: Vec<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rsi14_series: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orderbook: Option<OrderBookSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Default for CoinMarketData {
    fn default() -> Self {
        Self {
            current_price: None,
            current_ema20: None,
            current_ema50: None,
            current_macd: None,
            current_rsi: None,
            open_interest: None,
            funding_rate: None,
            price_series: Vec::new(),
            ema20_series: Vec::new(),
            ema50_series: Vec::new(),
            macd_series: Vec::new(),
            rsi7_series: Vec::new(),
            rsi14_series: Vec::new(),
            orderbook: None,
            notes: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(default)]
pub struct OpenInterestSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average: Option<f64>,
}

impl Default for OpenInterestSummary {
    fn default() -> Self {
        Self {
            latest: None,
            average: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(default)]
pub struct OrderBookSnapshot {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bids: Vec<OrderBookLevel>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub asks: Vec<OrderBookLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl Default for OrderBookSnapshot {
    fn default() -> Self {
        Self {
            bids: Vec::new(),
            asks: Vec::new(),
            timestamp: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(default)]
pub struct OrderBookLevel {
    pub price: f64,
    pub size: f64,
}

impl Default for OrderBookLevel {
    fn default() -> Self {
        Self {
            price: 0.0,
            size: 0.0,
        }
    }
}

pub async fn fetch_market_data(
    client: &OkxRestClient,
    request: &MarketDataRequest,
) -> Result<MarketDataResponse> {
    ensure!(!request.coins.is_empty(), "coins 不能为空");

    let timeframe = normalize_timeframe(&request.timeframe);
    let quote = normalize_quote(&request.quote);
    let indicator_set: HashSet<String> = request
        .indicators
        .iter()
        .map(|value| value.trim().to_lowercase())
        .collect();

    let mut response = MarketDataResponse {
        timestamp: Utc::now().to_rfc3339(),
        coins: BTreeMap::new(),
    };

    for coin in &request.coins {
        let inst_id = resolve_instrument_id(coin, &quote);

        match gather_coin_data(
            client,
            &inst_id,
            &timeframe,
            &indicator_set,
            request.include_orderbook,
            request.include_funding,
            request.include_open_interest,
        )
        .await
        {
            Ok(data) => {
                response.coins.insert(coin.to_uppercase(), data);
            }
            Err(err) => {
                warn!(error = ?err, coin = %coin, "failed to gather market data");
                let mut data = CoinMarketData::default();
                data.notes = Some(format!("数据拉取失败: {}", err));
                response.coins.insert(coin.to_uppercase(), data);
            }
        }
    }

    Ok(response)
}

async fn gather_coin_data(
    client: &OkxRestClient,
    inst_id: &str,
    timeframe: &str,
    indicators: &HashSet<String>,
    include_orderbook: bool,
    include_funding: bool,
    include_open_interest: bool,
) -> Result<CoinMarketData> {
    let mut data = CoinMarketData::default();

    // Price series & indicators
    let mut candles = client
        .get_candles(inst_id, timeframe, Some(DEFAULT_CANDLE_LIMIT))
        .await
        .with_context(|| format!("获取 {inst_id} K线失败"))?;

    if candles.is_empty() {
        return Err(anyhow!("{inst_id} 返回的 K 线数据为空"));
    }

    candles.sort_by_key(|c| c.timestamp);

    let price_series: Vec<f64> = candles.iter().map(|c| c.close).collect();
    let latest_price = price_series.last().copied();

    if indicators.is_empty() || indicators.contains("price") {
        data.price_series = price_series.clone();
        data.current_price = latest_price;
    } else {
        data.current_price = latest_price;
    }

    if indicators.contains("ema20") || indicators.contains("ema") {
        let ema20 = compute_ema(&price_series, 20);
        if !ema20.is_empty() {
            data.current_ema20 = ema20.last().copied();
            data.ema20_series = ema20;
        }
    }

    if indicators.contains("ema50") || indicators.contains("ema") {
        let ema50 = compute_ema(&price_series, 50);
        if !ema50.is_empty() {
            data.current_ema50 = ema50.last().copied();
            data.ema50_series = ema50;
        }
    }

    if indicators.contains("macd") {
        let macd_series = compute_macd(&price_series);
        if !macd_series.is_empty() {
            data.current_macd = macd_series.last().copied();
            data.macd_series = macd_series;
        }
    }

    let mut current_rsi = None;
    if indicators.contains("rsi7") || indicators.contains("rsi") {
        let rsi7 = compute_rsi(&price_series, 7);
        if !rsi7.is_empty() {
            data.rsi7_series = rsi7.clone();
            current_rsi = rsi7.last().copied().or(current_rsi);
        }
    }

    if indicators.contains("rsi14") || indicators.contains("rsi") {
        let rsi14 = compute_rsi(&price_series, 14);
        if !rsi14.is_empty() {
            data.rsi14_series = rsi14.clone();
            current_rsi = rsi14.last().copied().or(current_rsi);
        }
    }

    data.current_rsi = current_rsi;

    if include_orderbook {
        if let Ok(snapshot) = client.get_order_book(inst_id, Some(10)).await {
            data.orderbook = Some(convert_orderbook(snapshot));
        }
    }

    if include_funding {
        if let Ok(funding) = client.get_funding_rate(inst_id).await {
            data.funding_rate = parse_optional_float(funding.funding_rate.as_deref())
                .or_else(|| parse_optional_float(funding.next_funding_rate.as_deref()));
        }
    }

    if include_open_interest {
        if let Ok(oi) = client.get_open_interest(inst_id).await {
            let latest = parse_optional_float(oi.oi.as_deref());
            let average = parse_optional_float(oi.oi.as_deref());
            data.open_interest = Some(OpenInterestSummary { latest, average });
        }
    }

    if data.current_price.is_none() {
        if let Some(price) = latest_price {
            data.current_price = Some(price);
        }
    }

    if data.current_price.is_none() {
        data.notes = Some("缺少可用的价格数据".to_string());
    }

    Ok(data)
}

fn normalize_timeframe(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_TIMEFRAME.to_string()
    } else {
        trimmed.to_lowercase()
    }
}

fn normalize_quote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_QUOTE.to_string()
    } else {
        trimmed.to_uppercase()
    }
}

fn resolve_instrument_id(coin: &str, quote: &str) -> String {
    let cleaned = coin.trim().to_uppercase();
    if cleaned.contains('-') {
        cleaned
    } else {
        format!("{}-{}-SWAP", cleaned, quote)
    }
}

fn convert_orderbook(snapshot: OkxOrderBookSnapshot) -> OrderBookSnapshot {
    let OkxOrderBookSnapshot { asks, bids, ts } = snapshot;

    let bids = bids
        .into_iter()
        .filter_map(|level| parse_orderbook_level(level))
        .collect();
    let asks = asks
        .into_iter()
        .filter_map(|level| parse_orderbook_level(level))
        .collect();

    OrderBookSnapshot {
        bids,
        asks,
        timestamp: ts,
    }
}

fn parse_orderbook_level(level: Vec<String>) -> Option<OrderBookLevel> {
    if level.len() < 2 {
        return None;
    }
    let price = parse_optional_float(Some(level[0].as_str()))?;
    let size = parse_optional_float(Some(level[1].as_str()))?;
    Some(OrderBookLevel { price, size })
}

fn parse_optional_float(value: Option<&str>) -> Option<f64> {
    value.and_then(|raw| raw.parse::<f64>().ok())
}

fn compute_ema(series: &[f64], period: usize) -> Vec<f64> {
    if series.is_empty() || period == 0 || series.len() < period {
        return Vec::new();
    }

    let mut ema_values = Vec::with_capacity(series.len());
    let multiplier = 2.0 / (period as f64 + 1.0);

    let initial_avg: f64 = series[..period].iter().copied().sum::<f64>() / period as f64;
    for _ in 0..(period - 1) {
        ema_values.push(initial_avg);
    }
    ema_values.push(initial_avg);

    let mut prev_ema = initial_avg;
    for price in &series[period..] {
        let ema = (price - prev_ema) * multiplier + prev_ema;
        ema_values.push(ema);
        prev_ema = ema;
    }

    ema_values
}

fn compute_macd(series: &[f64]) -> Vec<f64> {
    if series.is_empty() {
        return Vec::new();
    }

    let ema12 = compute_ema(series, 12);
    let ema26 = compute_ema(series, 26);
    if ema12.is_empty() || ema26.is_empty() || ema12.len() != ema26.len() {
        return Vec::new();
    }

    ema12
        .into_iter()
        .zip(ema26.into_iter())
        .map(|(fast, slow)| fast - slow)
        .collect()
}

fn compute_rsi(series: &[f64], period: usize) -> Vec<f64> {
    if series.len() <= period || period == 0 {
        return Vec::new();
    }

    let mut rsis = vec![50.0; series.len()];
    let mut gains = 0.0;
    let mut losses = 0.0;

    for i in 1..=period {
        let delta = series[i] - series[i - 1];
        if delta >= 0.0 {
            gains += delta;
        } else {
            losses -= delta;
        }
    }

    let mut avg_gain = gains / period as f64;
    let mut avg_loss = losses / period as f64;

    rsis[period] = calculate_rsi(avg_gain, avg_loss);

    for i in (period + 1)..series.len() {
        let delta = series[i] - series[i - 1];
        let gain = if delta > 0.0 { delta } else { 0.0 };
        let loss = if delta < 0.0 { -delta } else { 0.0 };

        avg_gain = ((avg_gain * (period as f64 - 1.0)) + gain) / period as f64;
        avg_loss = ((avg_loss * (period as f64 - 1.0)) + loss) / period as f64;

        rsis[i] = calculate_rsi(avg_gain, avg_loss);
    }

    rsis
}

fn calculate_rsi(avg_gain: f64, avg_loss: f64) -> f64 {
    if avg_loss.abs() < f64::EPSILON {
        100.0
    } else {
        let rs = avg_gain / avg_loss;
        100.0 - (100.0 / (1.0 + rs))
    }
}
