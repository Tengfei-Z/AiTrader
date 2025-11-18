# 波动触发模块拆分设计

## 背景

目前 `backend/src/main.rs` 同时承担：

1. API 服务器启动/依赖注入；
2. 策略调度与波动触发逻辑；
3. OKX 行情轮询（含重试策略）的具体实现。

随着逻辑增多，`main.rs` 的职责高度耦合，难以：

- 独立测试波动触发算法（窗口、阈值、退避策略）；
- 后续扩充更多触发来源（例如链上数据）；
- 在 CLI/单元测试中复用同样的波动触发器。

因此需要将波动触发相关代码拆分成独立模块，明确责任边界。

## 设计目标

1. **解耦启动逻辑**：`main.rs` 只负责“是否启用波动触发”与注入依赖；
2. **集中业务逻辑**：波动触发模块负责行情轮询、重试、阈值检测和唤醒策略执行；
3. **易于配置**：复用现有 `CONFIG.okx_*` 参数，无需额外改动 `.env`；
4. **可测试**：模块暴露清晰的接口，便于注入 mock client/notify 进行单元测试；
5. **无行为回归**：功能、日志语义与现状保持一致。

## 模块结构

```
backend/src/
├── main.rs              // 仅保留调度入口
├── volatility_trigger/
│   ├── mod.rs           // 对外接口
│   └── runner.rs        // 具体实现（可按需要再拆）
```

### 模块接口

```rust
pub struct VolatilityTriggerConfig {
    pub poll_interval: Duration,
    pub threshold_bps: u64,
    pub window_secs: u64,
    pub max_attempts: usize,
    pub retry_backoff: Duration,
    pub symbols: Vec<String>,
}

pub fn spawn_volatility_trigger(
    client: OkxRestClient,
    notify: Arc<Notify>,
    config: VolatilityTriggerConfig,
) -> tokio::task::JoinHandle<()>;
```

- `spawn_volatility_trigger`：负责 `tokio::spawn` 内部轮询循环。`main.rs` 拿到 handle 后无需关心细节；
- `VolatilityTriggerConfig`：由 `main.rs` 按现有 `CONFIG` 构造，未来若引入动态刷新可拓展；
- 模块内部维护 `fetch_ticker_with_retry`、`should_retry_ticker_error` 等 helper，保持可见性在模块内。

### 数据流

1. `main.rs` 判断 `CONFIG.strategy_vol_trigger_enabled()`，若启用则：
   - `strategy_trigger::sync_symbol_states`（已存在）；
   - 构造 `VolatilityTriggerConfig`；
   - 调用 `spawn_volatility_trigger(client.clone(), wake_signal.clone(), cfg)`;
2. 模块中：
   - 使用 `tokio::time::interval(cfg.poll_interval)` 轮询；
   - 对每个 `cfg.symbols` 调用 `client.get_ticker`；
   - 失败时按 `max_attempts` + `retry_backoff` 重试；
   - 成功后调用 `strategy_trigger::record_tick_price`，若超过阈值则 `notify.notify_waiters()`；
   - 保留现有日志/错误处理语义。

## 迁移步骤

1. 新建 `src/volatility_trigger/mod.rs`，移动 `fetch_ticker_with_retry`、`should_retry_ticker_error`、`duration_from_secs` 及 `run_volatility_trigger_loop`；
2. 在新模块中定义 `VolatilityTriggerConfig` 和 `spawn_volatility_trigger`；
3. `main.rs`：
   - `mod volatility_trigger;`；
   - 构造配置并调用 `volatility_trigger::spawn_volatility_trigger`;
   - 移除原本的函数定义；
4. 调整 `use` 引入路径，确保 `OkxRestClient`、`Notify` 相关引用正确；
5. `cargo fmt && cargo check` 验证。

## 后续扩展

- 可在模块内部支持“不同数据源”（例如 WebSocket 缓存）或“不同窗口算法”；
- 可为模块添加单元测试（mock client，使用 `tokio::time::pause` 验证）；
- 若未来对外暴露 REST/CLI，则只需复用同一接口。
