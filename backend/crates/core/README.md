# ai_core

AiTrader 后端的公共工具库，专注于加载配置与复用的领域类型。

## 主要功能

- 通过 `AppConfig` 从环境变量加载配置，涵盖 OKX 凭证与 DeepSeek 设置。
- 通过全局的 `CONFIG` 懒加载，简化启动流程。
- 提供常用领域结构体，例如 `TradingPair`、`OrderSide`、`OrderType` 等。

## 测试方式

目前尚未添加单元测试，但可以通过编译保证质量：

```bash
cargo check -p ai_core
```

如需手动验证配置加载，可在本地设置 `OKX_*` 或 `DEEPSEEK_*` 环境变量，再在依赖该 crate 的测试或脚本中调用。
