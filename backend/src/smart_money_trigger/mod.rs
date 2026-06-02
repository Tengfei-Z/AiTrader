use crate::okx::models::Kline;
use crate::okx::OkxRestClient;
use crate::strategy_trigger::TriggerSource;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct SmartMoneyTriggerConfig {
    pub symbols: Vec<String>,
    pub poll_interval: Duration,
    pub signal_cooldown: Duration,
    pub max_attempts: u32,
    pub retry_backoff: Duration,
    pub bar: String, // e.g. "15m" or "1H"
    pub limit: u32,  // limit for fetching klines
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum TrendDir {
    Up,
    Down,
}

#[derive(Debug, Clone, Serialize)]
pub struct PriceZone {
    pub high: f64,
    pub low: f64,
    pub reason: String,
    pub is_support: bool,
}

impl PriceZone {
    pub fn contains(&self, price: f64) -> bool {
        price >= self.low && price <= self.high
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum SignalType {
    PotentialLong,
    PotentialShort,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmartMoneySignal {
    pub signal_type: SignalType,
    pub reason: String,
    pub micro_confirmation: String,
}

#[derive(Debug, Clone)]
struct RecentSignal {
    key: String,
    triggered_at: Instant,
}

fn detect_trend(klines: &[Kline]) -> Option<TrendDir> {
    if klines.len() < 4 {
        return None;
    }
    let last_close = klines.last()?.close;
    let anchor_close = klines.get(klines.len() - 4)?.close;
    if last_close > anchor_close {
        Some(TrendDir::Up)
    } else if last_close < anchor_close {
        Some(TrendDir::Down)
    } else {
        None
    }
}

/// 寻找流动性池 (Swing Highs & Swing Lows)
fn find_liquidity_pools(klines: &[Kline]) -> Vec<PriceZone> {
    let mut zones = Vec::new();
    let n = klines.len();
    if n < 5 {
        return zones;
    }

    // 简单找前高前低: 寻找局部极值点 (比如左右2根K线都比它低/高)
    let lookback = 2;
    for i in lookback..n - lookback {
        let current = &klines[i];

        // 找 Swing High
        let mut is_swing_high = true;
        for j in i - lookback..=i + lookback {
            if i != j && klines[j].high > current.high {
                is_swing_high = false;
                break;
            }
        }

        // 找 Swing Low
        let mut is_swing_low = true;
        for j in i - lookback..=i + lookback {
            if i != j && klines[j].low < current.low {
                is_swing_low = false;
                break;
            }
        }

        let buffer = current.close * 0.002; // 0.2% buffer zone

        if is_swing_high {
            zones.push(PriceZone {
                high: current.high + buffer,
                low: current.high - buffer,
                reason: format!("Previous Swing High at {}", current.timestamp),
                is_support: false, // 突破前高往往是假突破诱多，或者突破回踩
            });
        }

        if is_swing_low {
            zones.push(PriceZone {
                high: current.low + buffer,
                low: current.low - buffer,
                reason: format!("Previous Swing Low at {}", current.timestamp),
                is_support: true, // 跌破前低往往是假跌破诱空
            });
        }
    }

    zones
}

/// 是否为看涨吞没
fn is_bullish_engulfing(prev: &Kline, curr: &Kline) -> bool {
    // 前一根是阴线，当前是阳线
    let prev_is_bear = prev.close < prev.open;
    let curr_is_bull = curr.close > curr.open;

    // 当前阳线实体完全包住前一根阴线实体
    prev_is_bear && curr_is_bull && curr.open <= prev.close && curr.close >= prev.open
}

/// 是否为看跌吞没
fn is_bearish_engulfing(prev: &Kline, curr: &Kline) -> bool {
    let prev_is_bull = prev.close > prev.open;
    let curr_is_bear = curr.close < curr.open;

    prev_is_bull && curr_is_bear && curr.open >= prev.close && curr.close <= prev.open
}

/// 是否为长下影线 (Pinbar/Hammer)
fn is_bullish_pinbar(kline: &Kline) -> bool {
    let body = (kline.open - kline.close).abs();
    let lower_shadow = kline.open.min(kline.close) - kline.low;
    let upper_shadow = kline.high - kline.open.max(kline.close);

    // 下影线至少是实体的 2 倍，且上影线极短
    lower_shadow > body * 2.0 && upper_shadow < body
}

/// 是否为长上影线 (Shooting Star)
fn is_bearish_pinbar(kline: &Kline) -> bool {
    let body = (kline.open - kline.close).abs();
    let lower_shadow = kline.open.min(kline.close) - kline.low;
    let upper_shadow = kline.high - kline.open.max(kline.close);

    // 上影线至少是实体的 2 倍，且下影线极短
    upper_shadow > body * 2.0 && lower_shadow < body
}

/// 综合分析市场，并返回详细决策上下文日志。
pub fn analyze_market(klines: &[Kline]) -> (Option<SmartMoneySignal>, String) {
    let mut decision_steps: Vec<String> = Vec::new();
    if klines.len() < 5 {
        decision_steps.push(format!("insufficient_klines(len={})", klines.len()));
        return (None, decision_steps.join(" | "));
    }

    let last_idx = klines.len() - 1;
    let curr = &klines[last_idx];
    let prev = &klines[last_idx - 1];

    if let Some(trend) = detect_trend(klines) {
        decision_steps.push(format!("trend={trend:?}"));
    } else {
        decision_steps.push("trend=flat_or_unknown".to_string());
    }
    decision_steps.push(format!(
        "latest_ohlc(o={:.2},h={:.2},l={:.2},c={:.2},v={:.4})",
        curr.open, curr.high, curr.low, curr.close, curr.volume
    ));
    decision_steps.push(format!(
        "prev_ohlc(o={:.2},h={:.2},l={:.2},c={:.2})",
        prev.open, prev.high, prev.low, prev.close
    ));

    let current_price = curr.close;
    let pois = find_liquidity_pools(klines);
    decision_steps.push(format!("poi_detected={}", pois.len()));

    for (idx, poi) in pois.into_iter().enumerate() {
        // 条件1: 价格进入了POI
        let touched =
            poi.contains(current_price) || poi.contains(curr.low) || poi.contains(curr.high);
        if touched {
            decision_steps.push(format!(
                "poi_touched(index={},reason={},range=[{:.2},{:.2}],support={})",
                idx, poi.reason, poi.low, poi.high, poi.is_support
            ));
            let bullish_engulfing = is_bullish_engulfing(prev, curr);
            let bearish_engulfing = is_bearish_engulfing(prev, curr);
            let bullish_pinbar = is_bullish_pinbar(curr);
            let bearish_pinbar = is_bearish_pinbar(curr);
            decision_steps.push(format!(
                "pattern_check(bull_engulf={},bear_engulf={},bull_pinbar={},bear_pinbar={})",
                bullish_engulfing, bearish_engulfing, bullish_pinbar, bearish_pinbar
            ));

            // 底部/看涨共振
            if poi.is_support {
                if bullish_engulfing {
                    decision_steps.push("decision=trigger_long_by_bullish_engulfing".to_string());
                    return (
                        Some(SmartMoneySignal {
                            signal_type: SignalType::PotentialLong,
                            reason: poi.reason.clone(),
                            micro_confirmation: "看涨吞没形态 (Bullish Engulfing)".to_string(),
                        }),
                        decision_steps.join(" | "),
                    );
                } else if bullish_pinbar {
                    decision_steps.push("decision=trigger_long_by_bullish_pinbar".to_string());
                    return (
                        Some(SmartMoneySignal {
                            signal_type: SignalType::PotentialLong,
                            reason: poi.reason.clone(),
                            micro_confirmation: "看涨锤子线/长下影线 (Bullish Pinbar)".to_string(),
                        }),
                        decision_steps.join(" | "),
                    );
                }
            }
            // 顶部/看跌共振
            else {
                if bearish_engulfing {
                    decision_steps.push("decision=trigger_short_by_bearish_engulfing".to_string());
                    return (
                        Some(SmartMoneySignal {
                            signal_type: SignalType::PotentialShort,
                            reason: poi.reason.clone(),
                            micro_confirmation: "看跌吞没形态 (Bearish Engulfing)".to_string(),
                        }),
                        decision_steps.join(" | "),
                    );
                } else if bearish_pinbar {
                    decision_steps.push("decision=trigger_short_by_bearish_pinbar".to_string());
                    return (
                        Some(SmartMoneySignal {
                            signal_type: SignalType::PotentialShort,
                            reason: poi.reason.clone(),
                            micro_confirmation: "看跌流星线/长上影线 (Bearish Pinbar)".to_string(),
                        }),
                        decision_steps.join(" | "),
                    );
                }
            }
        }
    }

    decision_steps.push("decision=no_signal".to_string());

    (None, decision_steps.join(" | "))
}

pub fn spawn_smart_money_trigger(
    okx_client: OkxRestClient,
    wake_signal: Arc<Notify>,
    config: SmartMoneyTriggerConfig,
) {
    tokio::spawn(async move {
        info!(
            "启动 Smart Money (Liquidity + K-line) 分析触发器: {:?}",
            config.symbols
        );
        let mut recent_signals: HashMap<String, RecentSignal> = HashMap::new();
        loop {
            for symbol in &config.symbols {
                match process_symbol(&okx_client, symbol, &config).await {
                    Ok((Some(signal), decision_context)) => {
                        let signal_key = smart_money_signal_key(&signal);
                        let now = Instant::now();
                        if should_suppress_signal(
                            recent_signals.get(symbol),
                            &signal_key,
                            now,
                            config.signal_cooldown,
                        ) {
                            debug!(
                                "[{}] Smart Money 同类信号仍在冷却中，跳过触发: key={}, cooldown={}s",
                                symbol,
                                signal_key,
                                config.signal_cooldown.as_secs(),
                            );
                            continue;
                        }

                        info!(
                            "[{}] 发现 Smart Money 交易机会! signal={:?}, 宏观POI: {}, 微观形态: {}, 决策链路: {}",
                            symbol,
                            signal.signal_type,
                            signal.reason,
                            signal.micro_confirmation,
                            decision_context,
                        );
                        let trigger_context = format!(
                            "smart_money_trigger: signal={:?}; poi={}; confirmation={}; decision_context={}",
                            signal.signal_type,
                            signal.reason,
                            signal.micro_confirmation,
                            decision_context
                        );

                        // 更新全局 symbol 状态
                        if let Err(e) = crate::strategy_trigger::trigger_symbol(
                            symbol,
                            TriggerSource::SmartMoney,
                            None,
                            Some(trigger_context),
                        )
                        .await
                        {
                            error!("更新 symbol 状态失败 {}: {}", symbol, e);
                        } else {
                            recent_signals.insert(
                                symbol.clone(),
                                RecentSignal {
                                    key: signal_key,
                                    triggered_at: now,
                                },
                            );
                            // 唤醒主循环
                            wake_signal.notify_one();
                            debug!("[{}] 唤醒主策略循环", symbol);
                        }
                    }
                    Ok((None, decision_context)) => {
                        debug!(
                            "[{}] 未发现符合条件的买卖点，决策链路: {}",
                            symbol, decision_context
                        );
                    }
                    Err(e) => {
                        error!("[{}] 获取或分析 K 线失败: {}", symbol, e);
                    }
                }
            }
            sleep(config.poll_interval).await;
        }
    });
}

fn smart_money_signal_key(signal: &SmartMoneySignal) -> String {
    format!(
        "{:?}|{}|{}",
        signal.signal_type, signal.reason, signal.micro_confirmation
    )
}

fn should_suppress_signal(
    recent: Option<&RecentSignal>,
    signal_key: &str,
    now: Instant,
    cooldown: Duration,
) -> bool {
    let Some(recent) = recent else {
        return false;
    };
    recent.key == signal_key && now.duration_since(recent.triggered_at) < cooldown
}

async fn process_symbol(
    client: &OkxRestClient,
    symbol: &str,
    config: &SmartMoneyTriggerConfig,
) -> anyhow::Result<(Option<SmartMoneySignal>, String)> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match client
            .get_klines(symbol, &config.bar, Some(config.limit))
            .await
        {
            Ok(klines) => {
                return Ok(analyze_market(&klines));
            }
            Err(e) => {
                if attempts >= config.max_attempts {
                    return Err(anyhow::anyhow!(
                        "重试上限 ({}). 最后的错误: {}",
                        config.max_attempts,
                        e
                    ));
                }
                warn!(
                    "[{}] 获取 klines 失败: {}, {}秒后重试",
                    symbol,
                    e,
                    config.retry_backoff.as_secs()
                );
                sleep(config.retry_backoff).await;
            }
        }
    }
}
