# okx

AiTrader 使用的 OKX 交易所 Rust 客户端。该 crate 同时提供可复用的库接口（`OkxRestClient`）以及一个独立的 CLI 程序，方便进行冒烟测试。

## 主要特性

- 实现了 REST 请求签名（时间戳 + HMAC-SHA256 + Base64），并提供服务器时间查询示例。
- 支持通过 `ai_core::config::AppConfig` 自动加载配置。
- 提供 `okx-cli` 命令行工具，便于快速验证凭证与网络连通性。

## 使用方式

### 作为库

```rust
use ai_core::config::CONFIG;
use okx::OkxRestClient;

let client = OkxRestClient::from_config(&CONFIG)?;
let server_time = client.get_server_time().await?;
```

### 作为 CLI

先在当前目录创建 `.env`（或复制 `.env.example`）并填入凭证：

```bash
cp .env.example .env
vim .env  # 或使用你喜欢的编辑器
```

然后运行：

```bash
# 查询服务器时间
cargo run -p okx --bin okx-cli -- time

# 查询某个交易对的最新行情
cargo run -p okx --bin okx-cli -- ticker --symbol BTC-USDT

# 示例输出（以 BTC-USDT 为例）：
# {
#   "instId": "BTC-USDT",
#   "last": "112391.1",
#   "bidPx": "112391.1",
#   "askPx": "112391.2",
#   "high24h": "115590",
#   "low24h": "112084.7",
#   "vol24h": "8637.6433954",
#   "ts": "1761750515009"
# }
```

第二条命令会输出包含最新成交价、买一卖一、24 小时最高最低等信息的 JSON。

## 测试

目前仅保证可编译，可通过以下命令检查：

```bash
cargo check -p okx
```

后续补充更多 API 后，可在 `tests/` 或 `src/tests/` 中添加单元/集成测试，使用 `cargo test -p okx` 执行。
