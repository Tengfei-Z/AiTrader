# mcp_adapter

面向进程的 MCP（Model Context Protocol）工具适配层，用于在 AiTrader 后端与外部 MCP 工具之间建立通信。当前重点在于通过官方 [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk) SDK 启动 MCP Server 并与外部工具交互。

## 主要特性

- `DemoArithmeticServer` 演示如何用 `rmcp` 注册工具（`one_plus_one`、`get_account_state`），方便本地联调。
- 基于 `tokio` 实现，方便融入异步工作流。

## 使用方法

1. 确保本机已安装 Rust toolchain 以及 Node.js（以便使用 `npx`）。
2. 在 `backend/` 目录下运行 MCP Inspector，并让它替你启动 demo server：

   ```bash
   npx @modelcontextprotocol/inspector cargo run -p mcp_adapter --bin mcp-demo-server
   ```

   该命令会编译并运行 `mcp-demo-server`，随后在控制台输出一个本地调试页面（通常是 `http://localhost:3000` 一类的 URL）。

3. 在浏览器中打开该 URL，Transport 选择 `STDIO`，点击 “Connect”。
4. 连接成功后切换到 “Tools” 标签：
   - 调用 `one_plus_one` 验证基础通信是否畅通（预期返回 `2`）。
   - 调用 `get_account_state` 可获取当前 OKX 账户聚合信息，需确保 `.env` 中已配置 OKX API 凭证。

## 测试方式

目前仍以编译验证为主：

```bash
cargo check -p mcp_adapter
```

后续接入真实工具后，可在 `tests/` 中加入端到端用例，并通过 `cargo test -p mcp_adapter` 执行。
