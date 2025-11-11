# AiTrader 后端

这个目录现在只包含一个 Rust crate（`src/`），负责提供 REST API、加载配置以及访问 OKX。

## 架构概览

- `src/main.rs`：启动流程、全局 `AppState`、后台任务、路由汇总，将请求分发至 `routes/` 子模块。
- `src/routes/`：按功能拆路由。
  - `routes::market`：`/market/*` 行情接口 + DTO。
  - `routes::account`：`/account/*` 接口、余额快照循环和所有 OKX 数据逻辑。
  - `routes::model`：`/model/*` 策略对话与 `strategy-run` 触发。
- `src/agent_client.rs`：Rust → Python Agent HTTP 请求（`POST /analysis/`），保持同步调用层。
- `src/agent_subscriber.rs`：websocket client，复用 `AGENT_BASE_URL` 监听 `/agent/events/ws`，解析 `task_result` 并以 `db::upsert_agent_order` 更新数据库。
- `src/db.rs`：数据库初始化、迁移、查询以及 `orders`/`balances` 操作。
- `src/okx/`：OKX REST 客户端封装。
- `src/settings.rs` + `src/server_config.rs`：配置、环境变量、代理设置集中管理。
- `src/types.rs`：公共 `ApiResponse<T>` 响应结构。

## 运行服务

```bash
cd backend
cargo run -p api-server
```

服务启动前需要准备环境变量（可在仓库根目录复制 `backend/.env.example` 或直接使用统一的 `.env` 文件）：
- `OKX_*`：OKX 凭证（模拟/实盘由 `OKX_USE_SIMULATED` 控制）
- `AGENT_BASE_URL`：Python Agent WebSocket 端点（必须是 `ws://`，例如 `ws://localhost:8001/agent/events/ws`，Rust 会直接用这个 URL 建立与 `/agent/events/ws` 的连接）
- `DATABASE_URL`：PostgreSQL 连接字符串，开发时可在 `.env` 中写 `postgres://user:password@localhost:5432/aitrader`；部署脚本会把它传给 systemd。
- `DATABASE_SCHEMA`：PostgreSQL schema 名称，默认 `aitrader`，也可以通过 `.env` 覆盖；如果不设置，`init_database` 仍会使用 `aitrader`。

## 下一步建议

1. 在 `src/okx` 中扩充更多 REST 接口（账户、交易、WebSocket 订阅等）。
2. 将 Python Agent 的更多工具能力暴露为 REST 端点，并完善端到端测试。
3. 当接口稳定后，补充集成测试与 CI 工作流。
