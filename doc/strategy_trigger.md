# 策略触发改造需求

## 背景

当前策略触发机制由「固定间隔定时器」与「手动触发」组成，所有触发最终都会通过 `agent_subscriber::trigger_analysis()` 去调用 Python Agent。随着策略执行次数增多，定时轮询过于频繁且无法及时响应市场波动，需要引入“按行情波动自动触发”的能力，同时对触发调度进行统一管理。

## 功能点

1. **波动触发器**
   - 订阅 OKX 行情（WebSocket 或周期性 REST），每个 instId 维护 `last_trigger_price`。
   - 服务启动时先触发一次策略分析，成功或失败后都记录触发时行情价作为初始基准。
   - 运行中每当行情刷新，计算 `Δ = |price_now - last_trigger_price| / last_trigger_price`；若 Δ ≥ `STRATEGY_VOL_THRESHOLD_BPS`（默认 80bps = 0.8%），立即触发策略分析，并马上将定时兜底的下次执行时间延后一个完整周期。
   - 触发成功或失败后都要更新 `last_trigger_price`（失败时可根据错误类型决定是否刷新；推荐默认刷新，避免同一偏移反复触发）。
   - 可通过 `STRATEGY_VOL_TRIGGER_ENABLED` 控制是否启用。

2. **定时器整合**
   - `run_strategy_scheduler_loop` 改造为“最晚执行时间”模式：维护 `next_scheduled_at`，波动触发发生时立即更新为 `now + STRATEGY_SCHEDULE_INTERVAL_SECS`，并唤醒 scheduler 重新计算 sleep；定时器只在长时间无波动触发时兜底执行。
   - 为调度线程提供一个 `Notify` 或 `watch` 通道，使波动触发能安全地“推迟”下一次定时执行。
   - 调度日志要记录触发来源（volatility / schedule / manual），便于观察触发节奏。

3. **Agent 回包适配**
   - Agent 已会在分析失败时返回 `{"type":"analysis_error","error":...}`，Rust 需要在 `AgentMessage` 枚举中新增 `AnalysisError` 分支并在 `handle_agent_message` 中处理，确保 `trigger_analysis()` 能及时获知失败，而不是一直等待超时。
   - 无论结果成功或失败，触发调用方都要在 `trigger_analysis()` 返回后执行“更新基准价、刷新 next_scheduled_at（若来自波动触发）”等收尾操作。

4. **状态与容错**
   - 为每个 symbol 保存独立的 `SymbolState { last_trigger_price, last_trigger_at, next_scheduled_at, last_source }`，统一存放在 `HashMap<String, SymbolState>` 中。调度 loop 遍历 `CONFIG.okx_inst_ids()`，逐个 symbol 执行波动检测、定时兜底和状态更新。
   - 若触发过程中 WebSocket 尚未初始化，应记录告警并保持原基准价，由定时器/下次波动重新尝试；必要时可写入数据库或日志，便于进程重启后恢复状态。

5. **配置项**
   - `STRATEGY_VOL_TRIGGER_ENABLED`：是否启用波动触发（默认 false，逐步灰度）。
   - `STRATEGY_VOL_THRESHOLD_BPS`：波动阈值，单位基点（默认 80）。
   - `STRATEGY_SCHEDULE_INTERVAL_SECS`：兜底定时周期（建议调大到 900 秒以上）。
   - 未来如需保留独立冷却/窗口参数，可再扩展 `STRATEGY_VOL_WINDOW_SECS`（同一值同时作为观察窗口和冷却周期）。

## 交付期望

- 调度 loop 与波动触发共享 `ANALYSIS_PERMIT`，确保任何来源的策略执行都串行。
- 日志能够清晰说明“触发来源、触发时价格、基准价格、偏移百分比、结果（成功/失败）”。
- 配置缺省时系统行为与现状一致（只保留手动 + 固定定时），改造后无需强制更改部署脚本。
