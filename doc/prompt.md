# DeepSeek Prompt

This document captures the current prompt contract used by AiTrader when orchestrating DeepSeek function calls.

## System Prompt

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
5. get_performance_metrics：查看账户表现数据

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
```

The constant `DEFAULT_FUNCTION_CALL_SYSTEM_PROMPT` in `backend/crates/deepseek/src/client.rs` is the single source of truth. Components that need to extend the instructions (for example the CLI account-state helper) should append their own notes to this base prompt rather than duplicating it.

## User Prompt Template

Each DeepSeek invocation receives a structured payload summarised below.

```text
# 交易决策数据包

## 基本信息
- 当前时间：{current_timestamp}
- 交易开始时间：{start_time}
- 已运行时长：{elapsed_minutes} 分钟
- 调用次数：{invocation_count} 次
- 数据更新频率：每3分钟（部分指标可能使用不同周期）

## 市场状态数据
数据按 OLDEST → NEWEST 排序（时间序列从旧到新）

### BTC 数据
#### 当前状态
- 当前价格：${current_price}
- EMA20：${current_ema20}
- EMA50：${current_ema50}
- MACD：{current_macd}
- RSI(7)：{current_rsi7}
- RSI(14)：{current_rsi14}

#### 衍生品数据
- 开仓量（Open Interest）最新值与均值
- 资金费率（Funding Rate）

#### 3 分钟 K 线时间序列
- 价格、EMA20、EMA50、MACD、RSI(7)、RSI(14)

#### 高时间框架补充（部分币种可用）
- EMA20 vs. EMA50、ATR(3) vs. ATR(14)、当前成交量 vs. 均值
- MACD、RSI(14) 序列

### 其他币种（ETH、SOL、BNB、DOGE、XRP）
与 BTC 相同格式

## 账户状态
- 总资产、可用现金、总盈亏、手续费、已实现净收益
- 表现指标：Sharpe Ratio、胜率、平均杠杆、平均置信度、交易次数、最大盈利/亏损
- 持仓时间分布：多头、空头、空仓占比

## 当前持仓
- 对每个持仓给出方向、入场时间与价格、当前价格、数量、杠杆、强平价、保证金、未实现盈亏、名义价值
- 退出计划：止盈、止损、失效条件
- 若无持仓则明确说明

## 决策指引
1. 分析市场与趋势
2. 评估现有持仓的风险收益
3. 决定是否开仓、平仓、调整止盈止损或保持不变
4. 若交易，明确币种、方向、杠杆、资金量、退出计划
5. 给出决策置信度（0-100）
```

When wiring new components, ensure the JSON you send through `FunctionCallRequest.metadata["system_prompt"]` either reuses `DEFAULT_FUNCTION_CALL_SYSTEM_PROMPT` or extends it with additional notes to preserve consistent behaviour across the stack.
