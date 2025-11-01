# deepseek

 AiTrader 后端与 DeepSeek (OpenAI 兼容接口) 对接的封装，目前提供调用所需的数据结构与基于 `async-openai` 的异步客户端骨架。

## 主要内容

- `DeepSeekClient` 基于 `async-openai` 的 Chat Completions 接口，实现 `FunctionCaller` trait，可向 DeepSeek 发送函数调用请求。
- `FunctionCallRequest`/`FunctionCallResponse` 统一了序列化结构。
- 通过 `ai_core::config::AppConfig` 读取配置，避免在代码中写死密钥。

## 使用指南

1. 在当前目录复制并编辑 `.env`：

   ```bash
   cp .env.example .env
   vim .env  # 或使用你喜欢的编辑器
   ```

2. 使用 CLI：

   ```bash
   # 直接请模型评价当前 BTC 行情
   cargo run -p deepseek --bin deepseek-cli -- chat

    # 指定自定义提示词
   cargo run -p deepseek --bin deepseek-cli -- chat --prompt "分析 BTC 与 ETH 的相关性"

   # 调用函数接口并附带元数据（描述、参数 Schema 等）
   cargo run -p deepseek --bin deepseek-cli -- call --function test --arguments '{"foo":"bar"}' --metadata '{"description":"demo","parameters":{"type":"object"}}'
   ```

   `chat` 命令会打印模型的文本回复，`call` 命令会输出 JSON 结果，方便后续集成。

   示例输出（`chat`）：

   ```
   当前 BTC 价格维持震荡，下方支撑位约在 110000 美元附近，上方阻力在 115000 美元一带。短期可能继续盘整，若量能释放则有测试阻力的机会。

   风险提示：宏观数据及监管政策变化可能引发大幅波动，杠杆交易请谨慎控制仓位。
   ```

## 测试方式

当前以编译校验为主：

```bash
cargo check -p deepseek
```

后续补充实际接入后，可在 `tests/` 中添加集成测试，并通过 `cargo test -p deepseek` 执行。
