# AiTrader 后端

这个目录现在只包含一个 Rust crate（`src/`），负责提供 REST API、加载配置以及访问 OKX。

## 目录结构

- `src/main.rs`：Axum 入口，路由、状态管理、Agent 转发逻辑。
- `src/agent_client.rs`：与 Python Agent 交互的 HTTP 客户端封装。
- `src/server_config.rs`：读取 `config/config.yaml` 的可选绑定/代理配置。
- `src/settings.rs`：环境变量与 `.env` 加载，提供全局 `CONFIG`。
- `src/db.rs`：PostgreSQL 初始化与迁移工具（当前主要用于账户/策略相关表）。
- `src/okx/`：OKX REST 客户端、模型与签名逻辑。

## 运行服务

```bash
cd backend
cargo run -p api-server
```

服务启动前需要准备环境变量：
- `OKX_*` / `OKX_SIM_*`：OKX 主账户与模拟账户凭证
- `AGENT_BASE_URL`：Python Agent 服务地址（例如 `http://localhost:8001`）

## 下一步建议

1. 在 `src/okx` 中扩充更多 REST 接口（账户、交易、WebSocket 订阅等）。
2. 将 Python Agent 的更多工具能力暴露为 REST 端点，并完善端到端测试。
3. 当接口稳定后，补充集成测试与 CI 工作流。
