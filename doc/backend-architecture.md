# Backend Architecture

本文档描述 Rust 后端的模块分层，帮助快速理解各部分职责：

## 1. 模块划分

- `main.rs`
  - 负责启动流程：初始化日志、读取配置、初始化数据库、创建共享 `AppState`、注册 HTTP 路由与 CORS、启动后台任务。
  - 维持 `AppState`，它持有 Agent 客户端、OKX 模拟客户端与策略运行计数器，是所有 handler 的依赖源。
  - 通过 `routes::api_routes()` 构造 API 路由，将不同功能按路径组织。

- `routes/`
  - `routes::market`：提供 `/market/*` 所有行情接口，封装数据模型（`Ticker`/`OrderBook`/`Trade`）并复用 `ApiResponse` 模板。
  - `routes::account`：提供 `/account/*` 系列接口（余额、快照、仓位、历史、订单、成交），包含余额快照的周期任务 `run_balance_snapshot_loop`，所有 OKX 数据交互逻辑都在本模块。
  - `routes::model`：提供 `/model/*` 路径，负责暴露策略对话、触发策略执行，并在任务完成后落库。
  - `routes::mod.rs` 负责将子路由拼接，并把账户的事件循环导出给 `main.rs` 使用。

- `agent_client.rs`：封装 Rust → Python Agent 的 HTTP 调用（`POST /analysis/`），只负责同步调用，不包含业务逻辑。

- `agent_subscriber.rs`：起 WebSocket client，复用已有的 `AGENT_BASE_URL` 去连接 `/agent/events/ws`，接收 `task_result` 消息并调用 `db::upsert_agent_order`；同时负责对事件进行基本日志与错误处理。

- `db.rs`：数据库初始化、迁移与 CRUD；`orders`/`balances` 表结构定义与封装，以及 `upsert_agent_order` 等持久化逻辑。

- `okx/`：OKX REST 客户端封装，供 `routes` 模块使用（例如实时仓位、余额等）。

- `settings.rs` 与 `server_config.rs`：配置管理、环境变量解析与可选覆盖；`settings` 暴露 `CONFIG` 供全局共享。

- `types.rs`：抽象出 HTTP 规范的 `ApiResponse<T>`。

## 2. 背景任务

- `run_balance_snapshot_loop`（account 模块）每 5 秒拉取一次 OKX 余额，只有当估值变化超阈值时才写入 `balances` 表，保证有历史资产曲线。
- `agent_subscriber::run_agent_events_listener` 保持一条 websocket 连接来接收 Agent 的 `task_result`，并在收录后回复 `{"status":"received"}`。

## 3. 请求流程

1. 前端请求 `/model/strategy-run`，Rust 交给 `routes::model` 发起 `POST /analysis/`。
2. Agent 在完成 LLM 推理与 OKX 交互后将 `task_result` 通过 `/agent/events/ws` 推送给 Rust。
3. Rust 的 websocket client 解析事件，调用 `db::upsert_agent_order` 更新 `orders` 表。
4. 所有 HTTP 数据查询（余额、历史、持仓）仍由 Rust 的 `routes` 读取数据库或 OKX，保持单一数据源。

## 4. 未来演进建议

- 可以把每个 `routes` 模块进一步拆成 `handlers` + `serializers`，让测试更聚焦。
- 若需要更多背景任务（如行情同步、策略评估），建议放在 `services/` 级目录，并通过 `tokio::spawn` 在 `main.rs` 注册。
- 若 `routes/account` 的快照逻辑需要共享给其它模块，可将数据层抽成 `services::balances`，保持副作用集中。
