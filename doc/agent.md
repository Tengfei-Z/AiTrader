# AiTrader Agent 指南

本文档描述 AI 交易 Agent 的提示词（prompt）契约与 MCP 工具能力。后端以 `DeepSeek` 为主要推理引擎，`backend/crates/deepseek/src/client.rs` 中的 `DEFAULT_FUNCTION_CALL_SYSTEM_PROMPT` 是底层源码的单一事实来源；这里提供面向产品/Prompt 工程师的解释版，并补充 MCP 工具细节，方便快速对齐预期行为。

---

## 1. Prompt 契约

### 1.1 System Prompt（角色定义）

```text
你是一个专业的加密货币交易 AI，负责独立分析市场、制定交易计划并执行策略，目标是在控制风险的前提下最大化风险调整后的收益。

工作职责：
1. 产出 Alpha（洞察行情结构、识别交易机会、预测价格走势）
2. 决定仓位规模（资金分配、杠杆选择、风险敞口控制）
3. 安排交易节奏（判断开仓/平仓时机，设置止盈止损）
4. 落实风险管理（避免过度暴露，保持充足保证金与退出计划）

约束条件：
- 仅可交易白名单内的币种与合约
- 杠杆不超过 25X
- 每个持仓必须具备止盈、止损及失效条件
- 输出需清晰透明，便于审计与复盘

可用 MCP 工具：
1. get_market_data：获取实时行情及技术指标
2. get_account_state：查询账户状态与持仓
3. execute_trade：执行交易（开/平仓）
4. update_exit_plan：更新已有仓位的退出计划

输出要求（每次响应）：
1. 思考总结（≤200 字）：概述市场状况、持仓状态、下一步计划
2. 决策行动：如需操作，调用 MCP 工具并确保退出计划完整
3. 置信度（0-100）：给出当前判断的信心水平

策略提示：
- 风险优先，关注 Sharpe Ratio 等指标
- 避免无效频繁交易，留意成本
- 严格执行止损，保护本金
- 分散持仓，避免集中风险
- 顺势而为，尊重趋势
- 保持耐心，等待高质量信号
- 设置仓位规模时默认使用账户权益或可用保证金的 15%，可在 10%-25% 区间内按信号强弱调整，但需在输出中说明比例，避免名义资金占比低于 2% 的碎单
```

这一段在每次 DeepSeek 调用时注入，所有扩展场景都应在此基础上「追加」额外注意事项而非覆盖，确保 Agent 行为一致。

### 1.2 输出与响应格式

1. **思考总结**：≤200 字中文段落，解释趋势、波动、风险、仓位状态。
2. **工具调用**：如需开/平仓或调整计划，必须触发相应 MCP 工具，并且在自然语言说明退出计划。
3. **置信度**：`Confidence: 0-100` 数值结尾。

若判断无需操作，同样需要在总结中说明原因（如“信号不足”）并给出置信度。

### 1.3 用户 Prompt 模板

每次请求还附带结构化上下文，关键部分：

- **元信息**：当前时间、运行时长、调用次数。
- **市场数据包**：按币种拆分的指标（价格、EMA20/50、MACD、RSI7/14、资金费率、持仓量等），全部时间序列按「旧 → 新」排序。
- **账户状态**：权益、可用现金、累计盈亏、Sharpe、胜率、平均杠杆/置信度、交易次数、最大盈亏。
- **当前持仓**：方向、杠杆、数量、强平价、未实现盈亏、退出计划。
- **决策指引**：提醒分析市场 → 评估持仓 → 决定操作 → 输出置信度。

任何新入口（CLI、Cron、后台任务）在调用 DeepSeek 时都必须保证上述结构完整，以免 Agent 在缺失数据时做出偏差判断。

---

## 2. MCP 工具能力

所有工具实现位于 `backend/crates/mcp`，核心入口 `DemoArithmeticServer` 通过 `rmcp` 使用 stdio 与模型通讯。

### 2.1 `one_plus_one`
- **用途**：连通性测试，返回 `"2"`。
- **参数**：无。

### 2.2 `get_market_data`
- **功能**：拉取一个或多个合约的行情快照，生成价格序列、EMA、MACD、RSI、盘口、资金费率、持仓量等。
- **请求体**（`MarketDataRequest`）：
  - `coins: string[]`：如 `["BTC", "ETH"]`。未写 `-USDT-SWAP` 的会自动补全。
  - `timeframe: string`（固定 `3m`）：参数仅为兼容而保留，任何其他取值都会报错。
  - `quote: string`（默认 `USDT`）。
  - `indicators: string[]`：`price`、`ema20`、`macd`、`rsi7`。
  - `include_orderbook` / `include_funding` / `include_open_interest`: bool。
  - `simulated_trading: bool`：控制使用实盘/模拟盘凭证。
- **响应体**（`MarketDataResponse`）：以币种为 key，包含最新指标 (`current_price`, `current_ema20`, …)、序列数据、可选 `orderbook`、`funding_rate`、`open_interest`。
- **实现要点**：最多抓取 60 根 OKX K 线（约 3 小时的 3m 数据），按时间升序计算指标。缺失指标默认为 `null` 并在自然语言中解释。

### 2.3 `get_account_state`
- **功能**：读取 OKX 账户的权益与仓位概况。
- **请求参数**：
  - `simulated_trading: bool`
  - `include_positions: bool`（默认 `true`）
  - `include_history: bool`（默认 `false`，尚未实现）
  - `include_performance: bool`（默认 `false`，尚未实现）
- **响应**（`AccountState`）：
  - 整体权益、可用保证金。
  - `active_positions[]`：以 `coin`、`side`、`quantity`、`entry_price`、`margin_used`、`unrealized_pnl` 为主字段，缺失值省略。
  - 绩效指标位于 `performance_metrics`，目前多为 `null`，将逐步补齐。
- **依赖**：需配置 OKX REST API Key；内部调用 `/account/balance`、`/account/positions`、`/market/ticker`。

### 2.4 `execute_trade`
- **功能**：在 OKX 永续合约上下市价单，并可设置退出计划。
- **关键参数**（`ExecuteTradeRequest`）：
  - `action`: `"open_long" | "open_short" | "close_position"`
  - `coin` / `instrument_id`
  - `td_mode`（默认 `cross`）
  - `quantity` 或 `margin_amount + leverage`
  - `exit_plan`: `profit_target`、`stop_loss`, `invalidation_condition`
  - `simulated_trading`
- **响应**：`success`、`order_id`、`position_id`、`avg_price`、`filled_size`、`notional_value`、`liquidation_price`。
- **实现要点**：仅提交市价单；若设置 `exit_plan` 会调用 `/trade/set-trading-stop`，失败时记录日志但仍返回下单结果。

### 2.5 `update_exit_plan`
- **功能**：修改已有仓位的止盈/止损。
- **请求**（`UpdateExitPlanRequest`）：
  - `position_id`（`"<instId>[:posSide]"` 格式）或 `instrument_id`
  - `new_profit_target` / `new_stop_loss`（至少一个必填）
  - 可选 `new_invalidation`, `td_mode`, `simulated_trading`
- **响应**：更新后的触发价、方向提示。
- **实现要点**：再次调用 `/trade/set-trading-stop`；若未提供任何新目标，直接报错。

---

## 3. 扩展与开发建议

1. **新增 Prompt 约束**：在 DeepSeek 请求中通过 `metadata.system_prompt` 追加说明，切勿替换默认 System Prompt；必要时同步更新本文档以通知使用方。
2. **扩展 MCP 工具**：
   - 在 `backend/crates/mcp` 定义新的请求/响应结构，并派生 `JsonSchema` 方便 IDE 补全。
   - 使用 `#[tool]` 宏在 `DemoArithmeticServer` 注册。
   - 如需调用 OKX，优先在 `backend/crates/okx` 内加封装再被 MCP 使用，保持错误处理一致。
3. **回归测试**：提交前运行 `cargo test -p mcp_adapter`、`cargo test -p okx` 或相关集成测试，验证工具与提示词契约未被破坏。

当提示词或 MCP 有重要调整时，请同步在 PR 描述中引用 `doc/agent.md`，提醒前端/策略团队更新依赖流程。
