# mcp_adapter

面向进程的 MCP（Model Context Protocol）工具适配层，用于在 AiTrader 后端与外部 MCP 工具之间建立通信。当前重点在于启动外部进程、发送 JSON 请求以及异步读取响应。

## 主要特性

- `McpProcessHandle` 管理带管道的子进程 stdin/stdout。
- `McpRequest` / `McpResponse` 定义轻量级 JSON 消息格式。
- 基于 `tokio` 实现，方便融入异步工作流。

## 使用方法

1. 在当前目录准备 `.env`：

   ```bash
   cp .env.example .env
   vim .env  # 写入 MCP_EXECUTABLE、MCP_ARGS
   ```

2. 运行 CLI 手动发送请求：

   ```bash
   cargo run -p mcp_adapter --bin mcp-cli -- send --tool echo --payload '{"text":"hello"}'
   ```

   如需仅发送不等待响应，可追加 `--no-wait-response`。

## 测试方式

目前仍以编译验证为主：

```bash
cargo check -p mcp_adapter
```

后续接入真实工具后，可在 `tests/` 中加入端到端用例，并通过 `cargo test -p mcp_adapter` 执行。
