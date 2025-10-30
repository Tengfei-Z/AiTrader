# AiTrader 后端

这个工作区包含 AiTrader 的 Rust 后端代码。目录划分旨在让不同模块（交易所接入、AI 集成、工具链）彼此独立开发，同时共用核心类型与配置。

## Workspace 结构

- `crates/ai_core`：公共配置、领域类型与工具函数。
- `crates/okx`：OKX REST/WebSocket 客户端（已实现 REST 骨架）。
- `crates/deepseek`：DeepSeek Function Call 封装（结构就绪）。
- `crates/mcp`：MCP 工具适配层，包含进程管理的基础代码。
- `bins/trader-cli`：用于冒烟测试与手动操作的可执行程序。

## 运行 CLI

```bash
cd backend
cargo run -p trader-cli -- okx time
cargo run -p trader-cli -- okx ticker --symbol BTC-USDT
cargo run -p trader-cli -- deepseek chat
cargo run -p trader-cli -- deepseek call --function demo --arguments '{"foo":"bar"}'
cargo run -p trader-cli -- mcp send --tool echo --payload '{"text":"hello"}'
```

前两个命令演示 OKX API，需要在对应目录配置 `OKX_*` 凭证；中间两个演示 DeepSeek，需配置 `DEEPSEEK_*`；最后一个命令演示 MCP 进程交互，需要配置 `MCP_*`。

## 下一步建议

1. 在 `crates/okx` 中扩充更多 REST 接口（账户、交易、WebSocket 订阅等）。
2. 在 `crates/deepseek` 中完善 DeepSeek Function Call 流程。
3. 根据实际接入方案完善 `crates/mcp`。
4. 当接口稳定后，补充集成测试与 CI 工作流。
