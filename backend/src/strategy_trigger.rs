use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerSource {
    Scheduled,
    Manual,
    Volatility,
}

#[derive(Debug, Clone)]
pub struct PriceDeltaSnapshot {
    pub price_now: f64,
    pub base_price: f64,
    pub delta_bps: f64,
}

#[derive(Debug, Clone)]
pub struct SymbolTriggerState {
    pub next_scheduled_at: Instant,
    pub last_trigger_at: Option<Instant>,
    pub last_trigger_source: Option<TriggerSource>,
    pub last_trigger_price: Option<f64>,
    pub last_tick_price: Option<f64>,
    pub pending_trigger: Option<TriggerSource>,
}

impl SymbolTriggerState {
    fn new(now: Instant) -> Self {
        Self {
            next_scheduled_at: now,
            last_trigger_at: None,
            last_trigger_source: None,
            last_trigger_price: None,
            last_tick_price: None,
            pending_trigger: None,
        }
    }
}

static SYMBOL_STATES: Lazy<RwLock<HashMap<String, SymbolTriggerState>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 确保每个配置中的 symbol 都存在状态，未出现的状态会被移除。
pub async fn sync_symbol_states(symbols: &[String]) {
    let mut states = SYMBOL_STATES.write().await;
    let now = Instant::now();

    // 移除不在配置列表中的 symbol，避免状态泄漏
    if !states.is_empty() {
        let allowed: HashSet<&String> = symbols.iter().collect();
        states.retain(|symbol, _| allowed.contains(symbol));
    }

    for symbol in symbols {
        states
            .entry(symbol.clone())
            .or_insert_with(|| SymbolTriggerState::new(now));
    }
}

/// 返回当前需要执行的交易对及其触发来源。
pub async fn due_symbols(now: Instant, schedule_enabled: bool) -> Vec<(String, TriggerSource)> {
    let states = SYMBOL_STATES.read().await;
    states
        .iter()
        .filter_map(|(symbol, state)| {
            if let Some(source) = state.pending_trigger {
                return Some((symbol.clone(), source));
            }

            if schedule_enabled && state.next_scheduled_at <= now {
                return Some((symbol.clone(), TriggerSource::Scheduled));
            }

            None
        })
        .collect()
}

/// 计算下一次需要唤醒 scheduler 的时间点。
pub async fn next_due_instant() -> Option<Instant> {
    let states = SYMBOL_STATES.read().await;
    states.values().map(|state| state.next_scheduled_at).min()
}

/// 记录某个交易对一次触发完成，并延后下次调度时间。
pub async fn mark_trigger_completion(
    symbol: &str,
    interval: Duration,
    source: TriggerSource,
    last_price: Option<f64>,
) {
    let mut states = SYMBOL_STATES.write().await;
    let now = Instant::now();
    let next = now + interval;

    match states.get_mut(symbol) {
        Some(state) => {
            state.last_trigger_at = Some(now);
            state.last_trigger_source = Some(source);
            state.next_scheduled_at = next;
            state.pending_trigger = None;
            if let Some(price) = last_price {
                state.last_trigger_price = Some(price);
            }
            let remaining = next
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| Duration::from_secs(0))
                .as_secs_f64();
            debug!(%symbol, next_in = remaining, "recorded trigger completion");
        }
        None => {
            warn!(
                %symbol,
                "attempted to mark trigger completion for unknown symbol"
            );
        }
    }
}

/// 读取指定 symbol 的当前状态。
pub async fn get_symbol_state(symbol: &str) -> Option<SymbolTriggerState> {
    let states = SYMBOL_STATES.read().await;
    states.get(symbol).cloned()
}

/// 记录最新行情价，并在满足阈值时返回触发信息。
pub async fn record_tick_price(
    symbol: &str,
    price: f64,
    threshold_bps: u64,
    window_secs: u64,
) -> Option<PriceDeltaSnapshot> {
    let mut states = SYMBOL_STATES.write().await;
    let state = match states.get_mut(symbol) {
        Some(state) => state,
        None => {
            warn!(%symbol, "received ticker for unknown symbol");
            return None;
        }
    };

    state.last_tick_price = Some(price);

    if let (Some(last_at), Some(TriggerSource::Volatility)) =
        (state.last_trigger_at, state.last_trigger_source)
    {
        let window = Duration::from_secs(window_secs);
        if Instant::now().duration_since(last_at) < window {
            return None;
        }
    }

    let base_price = match state.last_trigger_price {
        Some(value) if value > 0.0 => value,
        _ => {
            state.last_trigger_price = Some(price);
            return None;
        }
    };

    if matches!(state.pending_trigger, Some(TriggerSource::Volatility)) {
        return None;
    }

    let delta_ratio = ((price - base_price) / base_price).abs();
    let delta_bps = delta_ratio * 10_000.0;
    if delta_bps < threshold_bps as f64 {
        return None;
    }

    state.pending_trigger = Some(TriggerSource::Volatility);
    state.next_scheduled_at = Instant::now();

    Some(PriceDeltaSnapshot {
        price_now: price,
        base_price,
        delta_bps,
    })
}

/// 计算当前价与基准价的偏移。
pub fn compute_price_delta(state: &SymbolTriggerState) -> Option<PriceDeltaSnapshot> {
    let price_now = state.last_tick_price?;
    let base_price = state.last_trigger_price?;
    if base_price == 0.0 {
        return None;
    }

    let delta_bps = ((price_now - base_price) / base_price).abs() * 10_000.0;
    Some(PriceDeltaSnapshot {
        price_now,
        base_price,
        delta_bps,
    })
}
