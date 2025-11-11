## Agent‑Rust WebSocket Design

### 1. 目标

- Restful agent→Rust 通信换成双向 WebSocket 连接，由 Rust 下发任务，agent 调用 OKX、收到响应后把 `ordId` 等字段沿原路返回；后续订单事件和值得持久化的信息由 agent 主动推送给 Rust。  
- Rust 保持对数据库（`orders` / `balances`）的控制，agent 只负责与外部交易所和大模型对接。

### 2. 总体架构

```text
Rust Task Scheduler
    ↕ (WebSocket)
Agent (OKX client + LLM bridge)
    ↔ OKX REST/WebSocket
    ↔ LLM/strategy pipeline (只读)
```

- Rust 发起 connection，agent 维持心跳、重连，确立 `session_id`/`agent_id`。  
- Rust 所有命令都通过 WebSocket `task_request` 消息下发；Agent 处理后立即通过 `task_result` 返回 `ordId` 并附带原始 OKX payload。  
- Agent 在接到 OKX WebSocket/Webhook 的执行、成交、平仓、收益事件时，推送 `order_event`/`pnl_update` 消息；Rust 反向写 DB 并触发对前端的通知。

### 3. 消息协议

#### 3.1. 任务下发（Rust → agent，启动策略/下单）

```json
{
  "type": "task_request",
  "task_id": "uuid",
  "payload": {
    "action": "place_order",
    "symbol": "BTC-USDT-SWAP",
    "side": "buy",
    "order_type": "limit",
    "price": "37000",
    "size": "1",
    "leverage": "10",
    "strategy_ids": ["..."]
  }
}
```

- `task_request` 代表 Rust 要 agent 执行的策略命令（如开启某个策略、下单/撤单），对应前面的 `strategies` 结论；`payload` 里可扩展 `strategy_id`、`confidence`、`stop_loss` 等辅助字段。
- Rust 下发 `place_order` 时，可把 `strategy_ids` 数组塞入，agent 只负责对 OKX 发起请求并返回结果，由 Rust 在 `orders` 表建立记录并追踪后续事件。
- Agent 可增加 `metadata` 里需要的模式示意：止盈/止损、算法版本、策略摘要等。

#### 3.2. 初始响应（agent → Rust）

```json
{
  "type": "task_result",
  "task_id": "uuid",     // 拿 task_request 关联
  "status": "accepted",
  "ordId": "3031...",
  "payload": { ... }    // 完整 OKX response，可直接存入 orders.metadata
}
```

- 如果下单失败，返回 `status: rejected` + 错误描述；Rust 根据 `task_id` 做异常处理。  
- agent 不能直接写 DB，只把 `ordId`/原始 `payload` 交给 Rust。

#### 3.3. 事件推送（agent → Rust）

- **order_event**：当 OKX 状态更新时（部分成交、撤单、完成），agent 发送 `{type: "order_event", ordId, status, filled_size, avg_px, event_ts}`。  
- **pnl_update**：收到 `closed-pnl` 等回执后发送 `{type: "pnl_update", ordId, realized_pnl, pnl_ts, instId}`。  
- **position_snapshot**：用于同步持仓/余额，可发送 `{type: "position_snapshot", positions: [...], balances: {...}}`，Rust 持久化到 `balances` 表或别的快照表。  
- 每条事件都附带 `agent_id`、`request_id` 等，便于追踪。

### 4. Rust 内部访问/写库建议

- Rust 接口层（如 `AgentClient` module）负责：保持 WebSocket 连接、发/收消息、重试、心跳（`ping`/`pong`）。  
- 收到 `task_result`/`order_event` 后更新 `aitrader.orders` 表：  
  - `id` 仍用内部 `uuid`；  
  - `metadata` 中存原始 OKX payload（包含 `ordId`）或策略上下文；  
  - `status`/`filled_size`/`closed_at` 根据事件刷新。  
- `pnl_update` 触发 `orders` 表 `realized_pnl` 字段（如果有）或单独的 `pnl_records` 扩展表。  
- `position_snapshot` 可写 `balances` 表（目前 schema 已定义 `valuation`, `available` 等字段）用于趋势图/收益图。

### 5. 可靠性 & 运维

- Agent 保持 `ping` 心跳；Rust 超时未收到回应应尝试重连并暂停新的任务。  
- 所有消息记录 `send_at`/`recv_at`、`req_id`、`agent_id` 便于追踪。  
- `task_request` 失败需返回明确错误代码并记录在 `orders.metadata` `agent_error` 里。  
- Rust 端每日可对比 OKX 的 `trade/fills` 或 `closed-pnl` 重新对账 `orders` 数据，必要时通过 agent 再次查询。

### 6. 迁移步骤建议

1. 先在 agent 中实现 WebSocket client/server 框架，并在测试环境做双向连接；保持旧 HTTP 接口，逐步引入 `task_request` 消息。  
2. Rust 增加 `AgentClient`，处理 `task_result`/`event` 消息的解析与 `orders` 更新逻辑。  
3. 验证 WebSocket 连接后，逐步禁用旧 HTTP 通道，只保留必要的 agent 管理面板或调试接口。  
4. 跨服务质量检查：在 `orders.metadata` 里记录 agent 发来的原始 payload、OKX 请求 ID、响应时间，方便后续排查。

如需，我可以再把消息 schema 写成 OpenAPI/Protobuf definitions，也可以补充前端如何消费 Rust 暴露的同步后端数据接口。 
