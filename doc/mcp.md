# MCP 工具说明

本文档用于说明 AiTrader 后端当前向大模型（如 DeepSeek、OpenAI GPT）开放的 MCP（Model Context Protocol）工具能力。

## 服务概览

- **模块位置**：`backend/crates/mcp`
- **入口结构体**：`DemoArithmeticServer`
- **通讯方式**：通过 `rmcp` 以 stdio 传输
- **对外说明**：`"AiTrader MCP server exposing arithmetic demo plus OKX account and trading tools."`

## 已开放的工具

### `one_plus_one`
- **用途**：返回 `1 + 1` 的结果。
- **调用方式**：无参数，返回文本 `"2"`，主要用于验证 MCP 通路是否工作正常。

### `get_market_data`
- **用途**：为单个合约生成行情快照，包含价格序列、技术指标、盘口、资金费率、持仓量等信息。
- **请求参数**（`MarketDataRequest`）：
  - `coins` (`string[]`)：如 `["BTC", "ETH"]`，若未指定完整合约名会自动扩展为 `<COIN>-USDT-SWAP`。
  - `timeframe` (`string`, 默认 `"3m"` )：OKX K 线周期，支持 `1m`、`15m`、`4H`、`1D` 等。
  - `quote` (`string`, 默认 `"USDT"` )：当仅填写 `coin` 时使用的计价货币。
  - `indicators` (`string[]`)：控制返回哪些指标序列，支持 `price`、`ema20`、`ema50`、`macd`、`rsi7`、`rsi14`，也可以使用 `ema`、`rsi` 这种泛配。
  - `include_orderbook` / `include_funding` / `include_open_interest` (`bool`)：决定是否附加盘口、资金费率、持仓量。
  - `simulated_trading` (`bool`, 默认 `false`)：决定使用实盘还是模拟盘凭证访问 OKX。
- **响应数据**（`MarketDataResponse`）：
  - 顶层 ISO 时间戳，以及按币种大写名称索引的行情结构。
  - `current_price`、`current_ema20`、`current_ema50`、`current_macd`、`current_rsi` 等最新指标。
  - 价格与指标序列：`price_series`、`ema20_series`、`ema50_series`、`macd_series`、`rsi7_series`、`rsi14_series`。
  - 可选 `orderbook`（盘口前十档）、`funding_rate`（资金费率）、`open_interest`（持仓量摘要）。
- **实现要点**：
  - 通过 `/market/candles` 获取最多 200 条 K 线，按时间升序计算指标。
  - 持仓量平均值暂时复用最新数据，后续可以补采样逻辑。

### `get_account_state`
- **用途**：拉取 OKX 账户的余额与在持仓位概况。
- **请求参数**（`AccountStateRequest`）：
  - `simulated_trading` (`bool`, 默认 `false`)
  - `include_positions` (`bool`, 默认 `true`)
  - `include_history` (`bool`, 默认 `false`，**暂未实现**)
  - `include_performance` (`bool`, 默认 `false`，**暂未实现**)
- **响应主要字段**（`AccountState`）：
  - `account_value`、`available_cash`
  - `active_positions[]`：每个仓位包含 `coin`、`side`、入场价、数量、杠杆、未实现盈亏等信息（有值才返回）。
  - 绩效指标（`total_fees`、`net_realized`、`sharpe_ratio` 等）目前返回 `null`，等待后续补充。
- **依赖**：需要配置 OKX REST 凭证，调用 `/account/balance`、`/account/positions` 以及逐合约 ticker。

### `execute_trade`
- **用途**：在 OKX 永续合约上开仓或平仓，可附带止盈止损计划。
- **请求参数**（`ExecuteTradeRequest`）：
  - `action`（`"open_long" | "open_short" | "close_position"`）
  - `coin` 或 `instrument_id`
  - `quote`（默认 `USDT`）
  - `td_mode`（默认 `cross`）
  - 通过 `quantity` 或 `margin_amount` + `leverage` 指定仓位规模
  - 可选 `exit_plan`（含 `profit_target` / `stop_loss` / `invalidation_condition`）
  - `simulated_trading`：选择实盘或模拟盘
- **响应数据**（`ExecuteTradeResponse`）：返回 `success`、生成的 `position_id`、`order_id`、成交价格、数量、名义价值、强平价等。
- **实现要点**：
  - 仅使用市价单（`ord_type = market`）。
  - 携带退出计划时会调用 `/trade/set-trading-stop` 设置止盈止损，失败会记录日志但仍返回下单结果。

### `update_exit_plan`
- **用途**：更新既有仓位的止盈止损。
- **请求参数**（`UpdateExitPlanRequest`）：
  - `position_id`（`"<instId>[:posSide]"` 格式）
  - `new_profit_target`、`new_stop_loss`（至少一个必填）
  - 可选 `instrument_id`、`td_mode`、`new_invalidation`
  - `simulated_trading`：选择实盘或模拟盘
- **响应数据**（`UpdateExitPlanResponse`）：返回更新后的价格、方向信息及提示信息。
- **实现要点**：内部调用 `/trade/set-trading-stop`，若未传入止盈/止损将直接报错。

## 尚未完成的能力

- `get_account_state` 的历史与绩效统计仍为空壳，`include_history`、`include_performance` 参数暂未生效，需要补全盈亏、费用、夏普率等计算。

## 如何扩展新的 MCP 工具

1. 在 `backend/crates/mcp` 内定义请求/响应结构体，保留 `JsonSchema` 派生以便自动生成文档。
2. 在 `DemoArithmeticServer` 上使用 `#[tool]` 宏注册处理函数。
3. 如需访问 OKX，先在 `backend/crates/okx` 中封装客户端方法并做好错误处理。
4. 更新本文件以及外部使用说明，确保大模型侧能及时同步最新能力。

> 提示：开发完成后建议执行 `cargo test -p mcp_adapter`、`cargo test -p okx` 或相关集成测试，确保 MCP 工具面保持健康。
