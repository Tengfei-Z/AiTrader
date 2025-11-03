# AiTrader 数据存储设计（初稿）

本文档汇总当前交易控制台需要持久化的数据、数据库选型以及核心表结构，后续版本可按上线阶段持续补充。目标读者为后端与数据工程同学，便于实现统一的数据访问层、为前端 API 和 MCP 工具提供稳定契约。

---

## 1. 设计目标

- **集中化**：将账户、持仓、成交、策略对话等数据统一落地，既能支持前端看板、MCP 工具查询也便于统计分析。
- **实时性**：保证交易相关数据写入无明显延迟，支持 1~5 秒内刷新。
- **审计可追溯**：保留原始事件或快照，方便回放策略决策与实际执行之间的差异。
- **可扩展**：考虑实盘与模拟账户的共存、横向扩容以及将来引入多交易所的场景。

---

## 2. 存储选型

| 类型                | 建议组件       | 主要用途                                                |
|---------------------|----------------|---------------------------------------------------------|
| OLTP 关系型数据库   | PostgreSQL 15+ | 账户、订单、成交、持仓、策略对话等事务性数据            |
| 时序 / 批量存储（可选） | Kafka / S3 / MinIO | 市场行情快照、模型日志原始流（用于回测、风控分析）        |

后续若行情吞吐增大，可独立部署时序库（如 InfluxDB/TimescaleDB）或直接落 Kafka 供离线分析使用；当前阶段 PostgreSQL 足以承担主要读写。

---

## 3. 数据域划分

1. **账户资产域**：账户信息、余额快照、风险指标。
2. **交易执行域**：订单、成交、当前持仓、历史持仓、下单置信度。
3. **策略对话域**：大模型对话、执行建议、人工反馈。
4. **行情/元数据域**：交易所、标的、行情快照/指标、资金费率、持仓量。
5. **绩效指标域**：账户/策略的 Sharpe、胜率、持仓时长等聚合指标。
6. **日志追踪域（预留）**：策略执行流水、风控告警、外部系统回执。

---

## 4. PostgreSQL 表设计

> 以下示例字段仅展示核心列，实际建表时可根据业务落地更多属性（如创建人、更新时间、软删除等）。所有表建议增加 `created_at TIMESTAMPTZ DEFAULT now()` 与 `updated_at TIMESTAMPTZ` 并通过触发器维护。

### 4.0 Schema 规范

- 所有业务表统一创建在 `aitrader` schema 下，不使用默认 `public`。实例化数据库后先执行：

```sql
CREATE SCHEMA IF NOT EXISTS aitrader AUTHORIZATION <db_owner>;
```

- 表命名风格：`aitrader.<业务域>_<实体>`（如 `aitrader.accounts`、`aitrader.positions_closed`）。
- 通用字段约定：
  - `id UUID PRIMARY KEY DEFAULT gen_random_uuid()`
  - `created_at TIMESTAMPTZ DEFAULT now()`
  - `updated_at TIMESTAMPTZ DEFAULT now()`
  - 如需软删除，追加 `deleted_at TIMESTAMPTZ`
- 若需 `gen_random_uuid()` 请启用 `pgcrypto` 扩展：`CREATE EXTENSION IF NOT EXISTS pgcrypto;`
- 枚举字段优先使用 PostgreSQL `ENUM` 或参考表（如 `aitrader.enum_side`）统一维护，避免字符串拼写导致脏数据。
- 公共维度（交易所、标的、策略）使用独立表建模，业务表通过外键引用，便于后续支持多交易所/多策略。

### 4.1 基础维度

#### `exchanges`
| 字段        | 类型        | 说明                         |
|-------------|-------------|------------------------------|
| `id`        | SERIAL PK   | 内部主键                     |
| `code`      | TEXT UNIQUE | 交易所代号，如 `OKX`         |
| `name`      | TEXT        | 展示名称                     |
| `region`    | TEXT        | 所属地区，可选               |
| `metadata`  | JSONB       | API 限速、交易品种等扩展信息 |

#### `instruments`
| 字段            | 类型        | 说明                                                     |
|-----------------|-------------|----------------------------------------------------------|
| `id`            | UUID PK     | 内部主键                                                 |
| `exchange_id`   | INT FK      | 引用 `exchanges`                                         |
| `symbol`        | TEXT        | 与交易所对齐的合约代码                                   |
| `base_asset`    | TEXT        | 标的资产                                                 |
| `quote_asset`   | TEXT        | 计价资产                                                 |
| `instrument_type` | TEXT     | `spot` / `swap` / `futures` 等                            |
| `tick_size`     | NUMERIC     | 价格精度                                                 |
| `sz_increment`  | NUMERIC     | 数量最小步长                                             |
| `status`        | TEXT        | `tradable` / `suspended`                                 |
| `metadata`      | JSONB       | 资金费率周期、交割合约信息等                             |

> MCP 工具 `get_market_data` 与未来的多交易所支持都依赖该维度表，可缓存行情或指标时作为主键使用。

### 4.2 账户与资产

#### `accounts`
| 字段              | 类型            | 说明                                  |
|-------------------|-----------------|---------------------------------------|
| `id`              | UUID PK         | 内部主键                              |
| `exchange_id`     | INT FK          | 引用 `exchanges`                      |
| `external_id`     | TEXT UNIQUE     | 交易所账号或模拟账号标识             |
| `owner_user_id`   | UUID            | 对应系统用户，便于权限控制            |
| `mode`            | TEXT            | `live` / `simulated`                  |
| `status`          | TEXT            | `active` / `disabled` 等              |
| `default_quote`   | TEXT            | 默认计价货币（MCP 推导使用）          |
| `metadata`        | JSONB           | 账户别名、描述、API 限速缓存等        |

> **约束**：`UNIQUE(exchange_id, external_id)` 确保同一交易所账号唯一；`INDEX(owner_user_id)` 支持多租户查询。

#### `balance_snapshots`
| 字段          | 类型        | 说明                                   |
|---------------|-------------|----------------------------------------|
| `id`          | BIGSERIAL PK|                                       |
| `account_id`  | UUID FK     | 引用 `accounts`                        |
| `asset`       | TEXT        | 资产符号，如 `USDT`                    |
| `available`   | NUMERIC     | 可用余额                               |
| `locked`      | NUMERIC     | 冻结余额                               |
| `valuation_usdt` | NUMERIC  | 折合 USDT 的估值                       |
| `as_of`       | TIMESTAMPTZ | 余额对应时间（用于快照排序）           |
| `source`      | TEXT        | `exchange` / `manual` / `derived`      |

> **索引建议**：`UNIQUE(account_id, asset, as_of)` 便于幂等写入；按需要对 `as_of` 做时间分区。

### 4.3 订单与成交

#### `orders`
| 字段           | 类型            | 说明                                             |
|----------------|-----------------|--------------------------------------------------|
| `id`           | UUID PK         | 内部订单 ID（便于跨交易所）                     |
| `account_id`   | UUID FK         | 对应账户                                        |
| `instrument_id`| UUID FK         | 引用 `instruments`，统一标的                      |
| `exchange_order_id` | TEXT      | 交易所订单号                                    |
| `symbol`       | TEXT            | 原始交易对字符串                                |
| `side`         | TEXT            | `buy` / `sell`                                  |
| `order_type`   | TEXT            | `limit` / `market` 等                            |
| `price`        | NUMERIC(20, 8)  | 限价单价格，可空                                 |
| `size`         | NUMERIC(20, 8)  | 下单数量                                         |
| `filled_size`  | NUMERIC(20, 8)  | 已成交数量                                       |
| `status`       | TEXT            | `open` / `partially_filled` / `filled` / `canceled` … |
| `time_in_force`| TEXT            | 可选                                            |
| `signal_id`    | UUID            | 引用策略建议或 MCP 调用 ID                      |
| `confidence`   | NUMERIC(5, 2)   | 模型置信度（0-100），来自 MCP 请求              |
| `created_at`   | TIMESTAMPTZ     | 下单时间                                         |
| `updated_at`   | TIMESTAMPTZ     | 状态更新时间                                     |

> **索引建议**：`(account_id, instrument_id, created_at DESC)`、`(signal_id)`。

#### `fills`
| 字段           | 类型            | 说明                                    |
|----------------|-----------------|-----------------------------------------|
| `id`           | UUID PK         | 内部成交 ID                             |
| `account_id`   | UUID FK         |                                         |
| `instrument_id`| UUID FK         | 引用 `instruments`                      |
| `order_id`     | UUID FK         | 引用 `orders`                           |
| `position_id`  | UUID            | 对应持仓记录（若有）                     |
| `exchange_fill_id` | TEXT       | 交易所成交编号                          |
| `symbol`       | TEXT            |                                         |
| `side`         | TEXT            |                                         |
| `price`        | NUMERIC(20, 8)  | 成交价                                  |
| `size`         | NUMERIC(20, 8)  | 成交量                                  |
| `fee`          | NUMERIC(20, 8)  | 手续费                                  |
| `pnl`          | NUMERIC(20, 8)  | 单笔已实现盈亏（若交易所返回）          |
| `confidence`   | NUMERIC(5, 2)   | 从订单继承的置信度（便于绩效统计）      |
| `timestamp`    | TIMESTAMPTZ     | 成交时间                                |
| `raw_payload`  | JSONB           | 交易所回执原文（便于审计）              |

> **索引建议**：`(account_id, instrument_id, timestamp DESC)`、`(order_id)`, `(position_id)`.

### 4.4 持仓数据

#### `positions_open`
| 字段            | 类型        | 说明                                      |
|-----------------|-------------|-------------------------------------------|
| `id`            | UUID PK     |                                           |
| `account_id`    | UUID FK     |                                           |
| `instrument_id` | UUID FK     | 引用 `instruments`                        |
| `symbol`        | TEXT        | 交易所原始标识                             |
| `side`          | TEXT        | `long` / `short` / `net`                  |
| `quantity`      | NUMERIC(20, 8) | 当前持仓张数                            |
| `avg_entry_price` | NUMERIC(20, 8) | 加权成本价                            |
| `notional_value` | NUMERIC(24, 8) | 名义价值                                |
| `leverage`      | NUMERIC(10, 2) | 当前杠杆                                  |
| `margin`        | NUMERIC(24, 8) | 占用保证金                                |
| `liquidation_price` | NUMERIC(20, 8) | 强平价（若有）                        |
| `unrealized_pnl` | NUMERIC(24, 8) | 未实现盈亏                              |
| `exit_plan`     | JSONB       | 当前止盈/止损计划（MCP `update_exit_plan`） |
| `last_signal_id`| UUID        | 最近一次模型信号 ID                        |
| `opened_at`     | TIMESTAMPTZ | 建仓时间                                  |
| `updated_at`    | TIMESTAMPTZ | 最近一次同步                              |

> **约束**：`UNIQUE(account_id, instrument_id, side)`，确保每个方向最多一条持仓记录。

#### `positions_closed`
用于填充前端“历史持仓”，也驱动盈亏统计。

| 字段            | 类型        | 说明                                         |
|-----------------|-------------|----------------------------------------------|
| `id`            | UUID PK     |                                              |
| `account_id`    | UUID FK     |                                              |
| `instrument_id` | UUID FK     |                                              |
| `symbol`        | TEXT        |                                              |
| `side`          | TEXT        |                                              |
| `quantity`      | NUMERIC(20, 8) | 平仓数量                                 |
| `entry_price`   | NUMERIC(20, 8) | 开仓均价                                 |
| `exit_price`    | NUMERIC(20, 8) | 平仓均价                                 |
| `leverage`      | NUMERIC(10, 2) | 开仓时杠杆                               |
| `margin`        | NUMERIC(24, 8) | 保证金                                    |
| `realized_pnl`  | NUMERIC(24, 8) | 已实现盈亏                                |
| `holding_minutes` | NUMERIC(14, 4) | 持仓时长（分钟）                         |
| `average_confidence` | NUMERIC(5, 2) | 持仓期间平均置信度                      |
| `entry_time`    | TIMESTAMPTZ | 入场时间                                    |
| `exit_time`     | TIMESTAMPTZ | 离场时间                                    |
| `source`        | TEXT        | `exchange` / `simulator` / `manual` 等       |
| `raw_payload`   | JSONB       | 交易所返回的原始记录                         |

> 平仓检测策略：接受 `fills` 流后更新 `positions_open`，当仓位归零时写入 `positions_closed`，并计算置信度平均值与持仓时长（为 MCP 性能指标提供数据）。

### 4.5 策略对话与操作记录

模型输出和人工反馈同样需要落库，便于追踪策略建议与执行结果。

#### `strategy_sessions`
| 字段          | 类型        | 说明                                       |
|---------------|-------------|--------------------------------------------|
| `id`          | UUID PK     | 会话 ID                                    |
| `account_id`  | UUID FK     | 对应账户或策略主体                         |
| `title`       | TEXT        | 可选：会话主题                             |
| `status`      | TEXT        | `active` / `closed`                        |
| `created_at`  | TIMESTAMPTZ | 会话开启时间                               |
| `closed_at`   | TIMESTAMPTZ | 结束时间                                   |
| `metadata`    | JSONB       | 额外上下文，如策略版本、触发条件           |

#### `strategy_messages`
| 字段          | 类型        | 说明                                         |
|---------------|-------------|----------------------------------------------|
| `id`          | UUID PK     |                                              |
| `session_id`  | UUID FK     | 引用 `strategy_sessions`                     |
| `role`        | TEXT        | `assistant` / `user` / `system`              |
| `content`     | TEXT        | 大模型或用户发言正文                         |
| `summary`     | TEXT        | 可选：提炼要点                              |
| `tags`        | TEXT[]      | 分类标签（如 `策略`, `风控`）                |
| `confidence`  | NUMERIC(5, 2) | 模型置信度，可选                            |
| `created_at`  | TIMESTAMPTZ | 消息时间                                     |
| `attachments` | JSONB       | 关联的订单草稿、行情快照等                   |

#### `mcp_tool_calls`
| 字段          | 类型        | 说明                                                |
|---------------|-------------|-----------------------------------------------------|
| `id`          | UUID PK     | MCP 调用 ID，对应 `signal_id`                       |
| `account_id`  | UUID FK     |                                                      |
| `session_id`  | UUID FK     | 可选，若来自策略会话                               |
| `tool_name`   | TEXT        | 如 `get_market_data` / `execute_trade`              |
| `request_payload`  | JSONB | 原始入参（脱敏后存储）                               |
| `response_payload` | JSONB | 返回结果，方便排错                                   |
| `status`      | TEXT        | `success` / `failed` / `pending`                   |
| `latency_ms`  | INTEGER     | 调用耗时                                            |
| `created_at`  | TIMESTAMPTZ | 调用时间                                            |

#### `strategy_actions`（可选）
记录模型建议与实际执行的映射，以便审计。

| 字段          | 类型        | 说明                                                |
|---------------|-------------|-----------------------------------------------------|
| `id`          | UUID PK     |                                                     |
| `message_id`  | UUID FK     | 来源消息                                            |
| `tool_call_id`| UUID FK     | 引用 `mcp_tool_calls`                              |
| `order_id`    | UUID FK     | 关联成功下发的订单                                 |
| `status`      | TEXT        | `pending` / `executed` / `rejected`                |
| `reason`      | TEXT        | 失败或拒绝原因                                     |
| `created_at`  | TIMESTAMPTZ |                                                     |

### 4.6 行情与快照（可选）

#### `market_snapshots`
| 字段             | 类型        | 说明                                                   |
|------------------|-------------|--------------------------------------------------------|
| `id`             | BIGSERIAL PK|                                                        |
| `instrument_id`  | UUID FK     | 引用 `instruments`                                    |
| `timeframe`      | TEXT        | `1m` / `3m` / `1H` …                                  |
| `as_of`          | TIMESTAMPTZ | K 线收盘时间                                           |
| `price`          | NUMERIC(20, 8) | 最新价格                                           |
| `ema20`          | NUMERIC(20, 8) | 指标缓存                                           |
| `ema50`          | NUMERIC(20, 8) |                                                      |
| `macd`           | NUMERIC(20, 8) |                                                      |
| `rsi7`           | NUMERIC(8, 4)  |                                                      |
| `rsi14`          | NUMERIC(8, 4)  |                                                      |
| `funding_rate`   | NUMERIC(10, 8) | 最近资金费率                                        |
| `open_interest`  | NUMERIC(24, 4) | 最新持仓量                                          |
| `open_interest_avg` | NUMERIC(24, 4) | 滚动平均持仓量                                  |
| `orderbook_top`  | JSONB       | 前 N 档盘口（可仅存档买卖一档）                       |
| `created_at`     | TIMESTAMPTZ | 插入时间                                               |

> `get_market_data` 可优先读取此表作为缓存，当数据过期时再请求 OKX。

#### `market_ticks`
存储更高频的逐笔或分钟级行情，可按 `instrument_id` + `ts` 建立分区。

#### `equity_snapshots`
| 字段          | 类型        | 说明                                 |
|---------------|-------------|--------------------------------------|
| `id`          | BIGSERIAL PK|                                      |
| `account_id`  | UUID FK     |                                      |
| `equity`      | NUMERIC(24, 8) | 账户权益                           |
| `cash`        | NUMERIC(24, 8) | 余额                               |
| `net_position_value` | NUMERIC(24, 8) | 净仓位价值                |
| `unrealized_pnl` | NUMERIC(24, 8) | 未实现盈亏                |
| `as_of`       | TIMESTAMPTZ | 快照时间                             |
| `source`      | TEXT        | `exchange` / `calculated`            |

---

### 4.7 绩效指标

为 MCP `get_account_state` 与 `get_performance_metrics` 提供聚合数据。

#### `performance_snapshots`
| 字段          | 类型        | 说明                                          |
|---------------|-------------|-----------------------------------------------|
| `id`          | BIGSERIAL PK|                                               |
| `account_id`  | UUID FK     |                                               |
| `window`      | TEXT        | 统计窗口：`daily` / `weekly` / `all_time`    |
| `sharpe_ratio`| NUMERIC(10, 6) |                                             |
| `win_rate`    | NUMERIC(6, 4)  | 胜率                                        |
| `average_leverage` | NUMERIC(10, 4) | 平均杠杆                            |
| `average_confidence` | NUMERIC(5, 2) | 平均置信度                         |
| `biggest_win` | NUMERIC(24, 8) | 最大单笔盈利                              |
| `biggest_loss`| NUMERIC(24, 8) | 最大单笔亏损                              |
| `hold_ratio_long` | NUMERIC(6, 4) | 多头持仓时间占比                      |
| `hold_ratio_short`| NUMERIC(6, 4) | 空头持仓时间占比                      |
| `hold_ratio_flat` | NUMERIC(6, 4) | 空仓时间占比                            |
| `updated_at`  | TIMESTAMPTZ | 最近一次刷新                               |

#### `performance_events`
可选，用于存储每日收益、手续费等时间序列，为 Sharpe 等指标提供原始数据。

#### `confidence_journal`
| 字段          | 类型        | 说明                                 |
|---------------|-------------|--------------------------------------|
| `id`          | BIGSERIAL PK|                                      |
| `signal_id`   | UUID        | 引用 `mcp_tool_calls`                |
| `account_id`  | UUID FK     |                                      |
| `confidence`  | NUMERIC(5, 2) | 当次下单置信度                      |
| `applied_order_id` | UUID FK | 实际下单引用                        |
| `created_at`  | TIMESTAMPTZ | 记录时间                              |

> 绩效刷新可以通过定时任务聚合 `positions_closed`、`fills`、`confidence_journal`，再写入 `performance_snapshots`，供 MCP 快速查询。

---

## 5. 表关系（ER 简述）

- `exchanges` ←→ `instruments`（1:N）
- `accounts` ←→ `balance_snapshots`（1:N）
- `accounts` ←→ `orders` ←→ `fills`
- `orders` ←→ `mcp_tool_calls`（1:N，通过 `signal_id`）
- `accounts` ←→ `positions_open` / `positions_closed`
- `accounts` ←→ `performance_snapshots`
- `accounts` ←→ `strategy_sessions` ←→ `strategy_messages`
- `strategy_messages` ←→ `strategy_actions`（可选 1:1）

所有交易域表均通过 `account_id` 绑定，方便区分实盘与模拟账户。若未来存在多交易所，可在对应表补充 `exchange` 字段或拆分 schema。

---

## 6. 同步与写入流程

1. **行情/账户拉取**：Connector 定期调用 OKX REST / WebSocket，同步余额、活跃持仓、成交，同时写入 `market_snapshots`（失效后再刷新）。
2. **事件处理**：数据进入处理器（Rust Service），更新 `orders`、`fills`、`positions_open`，归零后写 `positions_closed` 并记录置信度到 `confidence_journal`。
3. **策略对话**：大模型回复与 MCP 调用分别落 `strategy_messages`、`mcp_tool_calls`，若执行成功则在 `strategy_actions` 里记录关联的订单。
4. **绩效聚合**：定时任务读取 `fills`、`positions_closed`、`equity_snapshots`，计算指标写回 `performance_snapshots`。
5. **前端/MCP 查询**：控制台与 MCP 工具直接读取上述表，避免重复计算；需要实时数据时才回源交易所。
6. **快照归档**：`balance_snapshots` / `equity_snapshots` / 高频 `market_ticks` 可按日批量归档或迁移至对象存储。

---

## 7. 索引与性能建议

- 常规索引：对 `account_id`、`instrument_id`、`timestamp/created_at` 等常用查询字段建立联合索引。
- 分区策略：大体量表（如 `fills`, `strategy_messages`）按月份或季度分区，避免单表膨胀。
- JSONB 字段需结合 GIN 索引按需建立（如 `raw_payload`、`exit_plan`）。
- `market_snapshots` / `performance_events` 可使用 TimescaleDB hypertable 或 PostgreSQL 分区以提升写入与查询效率。

---

## 8. 数据保留与清理

| 数据类型          | 建议保留周期       | 处理方式                         |
|-------------------|--------------------|----------------------------------|
| 订单/成交/持仓    | 长期保留           | 只做分区与归档                   |
| 策略对话          | ≥ 1 年（便于审计） | 可按会话归档至对象存储           |
| 行情快照          | 30~90 天热存储     | 冷数据落 Kafka / S3              |
| 余额快照          | ≥ 180 天           | 过期数据按月归档或聚合           |
| 绩效快照          | ≥ 1 年             | 可定期聚合后只保留最近窗口       |

---

## 9. 风险与注意事项

1. **策略对话/工具调用敏感信息**：`strategy_messages`、`mcp_tool_calls` 中需脱敏 API 密钥、指令原文等敏感字段，并做行级访问控制。
2. **多账户/多交易所兼容**：提前保留 `exchange_id`、`strategy_id` 字段，避免未来扩容导致迁移难度大。
3. **一致性**：面对交易所接口异常时，需记录补偿任务，将缺失的订单/成交补齐，保持 `positions_open` 与实际一致。
4. **Schema 变更**：建议采用迁移工具（如 `sqlx migrate`/`diesel`），同步更新该文档。

---

## 10. 后续工作

- 根据此文档编写正式 SQL 建表脚本。
- 在 API 层加入分页、过滤参数映射对应表结构。
- 将模型会话的读取/写入接口纳入后端服务，并替换前端 mock 数据。
- 构建行情/绩效聚合任务，定期刷新 `market_snapshots` 与 `performance_snapshots`。
- 若引入 Kafka，规划事件溯源与回放机制，支持更精细的策略回测。

欢迎在需求发生变化时补充本文件，确保前后端与数据团队对齐。

---

## 附录：示例建表脚本（摘录）

以下 SQL 展示核心实体的基础结构，所有对象均创建在 `aitrader` schema 中，落地时可在此基础上增加索引、约束与触发器。

```sql
-- 先确保 schema 与扩展存在
CREATE ROLE aitrader_owner LOGIN PASSWORD '<password>';
CREATE SCHEMA IF NOT EXISTS aitrader AUTHORIZATION aitrader_owner;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- 账户信息
-- 交易所与标的
CREATE TABLE IF NOT EXISTS aitrader.exchanges (
    id          SERIAL PRIMARY KEY,
    code        TEXT UNIQUE NOT NULL,
    name        TEXT NOT NULL,
    region      TEXT,
    metadata    JSONB DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS aitrader.instruments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    exchange_id     INT NOT NULL REFERENCES aitrader.exchanges (id),
    symbol          TEXT NOT NULL,
    base_asset      TEXT NOT NULL,
    quote_asset     TEXT NOT NULL,
    instrument_type TEXT NOT NULL,
    tick_size       NUMERIC(20, 8) NOT NULL,
    sz_increment    NUMERIC(20, 8) NOT NULL,
    status          TEXT NOT NULL DEFAULT 'tradable',
    metadata        JSONB DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_instruments_exchange_symbol
    ON aitrader.instruments (exchange_id, symbol);

CREATE TABLE IF NOT EXISTS aitrader.accounts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    exchange_id     INT NOT NULL REFERENCES aitrader.exchanges (id),
    external_id     TEXT NOT NULL,
    owner_user_id   UUID,
    mode            TEXT NOT NULL CHECK (mode IN ('live', 'simulated')),
    status          TEXT NOT NULL DEFAULT 'active',
    default_quote   TEXT,
    metadata        JSONB DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (exchange_id, external_id)
);

-- 余额快照
CREATE TABLE IF NOT EXISTS aitrader.balance_snapshots (
    id              BIGSERIAL PRIMARY KEY,
    account_id      UUID NOT NULL REFERENCES aitrader.accounts (id),
    asset           TEXT NOT NULL,
    available       NUMERIC(24, 8) NOT NULL,
    locked          NUMERIC(24, 8) NOT NULL DEFAULT 0,
    valuation_usdt  NUMERIC(24, 8),
    source          TEXT NOT NULL DEFAULT 'exchange',
    as_of           TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, asset, as_of)
);
CREATE INDEX IF NOT EXISTS idx_balance_snapshots_account_asset
    ON aitrader.balance_snapshots (account_id, asset, as_of DESC);

-- 订单与成交
CREATE TABLE IF NOT EXISTS aitrader.orders (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id          UUID NOT NULL REFERENCES aitrader.accounts (id),
    instrument_id       UUID NOT NULL REFERENCES aitrader.instruments (id),
    exchange_order_id   TEXT,
    symbol              TEXT NOT NULL,
    side                TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    order_type          TEXT NOT NULL,
    price               NUMERIC(20, 8),
    size                NUMERIC(20, 8) NOT NULL,
    filled_size         NUMERIC(20, 8) NOT NULL DEFAULT 0,
    status              TEXT NOT NULL,
    time_in_force       TEXT,
    signal_id           UUID,
    confidence          NUMERIC(5, 2),
    metadata            JSONB DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_orders_account_instrument
    ON aitrader.orders (account_id, instrument_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_orders_signal_id
    ON aitrader.orders (signal_id);

CREATE TABLE IF NOT EXISTS aitrader.fills (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id          UUID NOT NULL REFERENCES aitrader.accounts (id),
    instrument_id       UUID NOT NULL REFERENCES aitrader.instruments (id),
    order_id            UUID NOT NULL REFERENCES aitrader.orders (id),
    position_id         UUID,
    exchange_fill_id    TEXT,
    symbol              TEXT NOT NULL,
    side                TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    price               NUMERIC(20, 8) NOT NULL,
    size                NUMERIC(20, 8) NOT NULL,
    fee                 NUMERIC(20, 8) NOT NULL DEFAULT 0,
    pnl                 NUMERIC(24, 8),
    confidence          NUMERIC(5, 2),
    timestamp           TIMESTAMPTZ NOT NULL,
    raw_payload         JSONB DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_fills_account_instrument
    ON aitrader.fills (account_id, instrument_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_fills_position
    ON aitrader.fills (position_id);

-- 当前持仓
CREATE TABLE IF NOT EXISTS aitrader.positions_open (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id          UUID NOT NULL REFERENCES aitrader.accounts (id),
    instrument_id       UUID NOT NULL REFERENCES aitrader.instruments (id),
    symbol              TEXT NOT NULL,
    side                TEXT NOT NULL,
    quantity            NUMERIC(20, 8) NOT NULL,
    avg_entry_price     NUMERIC(20, 8),
    notional_value      NUMERIC(24, 8),
    leverage            NUMERIC(10, 2),
    margin              NUMERIC(24, 8),
    liquidation_price   NUMERIC(20, 8),
    unrealized_pnl      NUMERIC(24, 8),
    exit_plan           JSONB DEFAULT '{}'::jsonb,
    last_signal_id      UUID,
    opened_at           TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, instrument_id, side)
);

-- 历史持仓
CREATE TABLE IF NOT EXISTS aitrader.positions_closed (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id          UUID NOT NULL REFERENCES aitrader.accounts (id),
    instrument_id       UUID NOT NULL REFERENCES aitrader.instruments (id),
    symbol              TEXT NOT NULL,
    side                TEXT NOT NULL,
    quantity            NUMERIC(20, 8) NOT NULL,
    entry_price         NUMERIC(20, 8),
    exit_price          NUMERIC(20, 8),
    leverage            NUMERIC(10, 2),
    margin              NUMERIC(24, 8),
    realized_pnl        NUMERIC(24, 8),
    holding_minutes     NUMERIC(14, 4),
    average_confidence  NUMERIC(5, 2),
    entry_time          TIMESTAMPTZ,
    exit_time           TIMESTAMPTZ NOT NULL,
    source              TEXT DEFAULT 'exchange',
    raw_payload         JSONB DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_positions_closed_account_exit
    ON aitrader.positions_closed (account_id, exit_time DESC);

-- MCP 调用日志
CREATE TABLE IF NOT EXISTS aitrader.mcp_tool_calls (
    id                  UUID PRIMARY KEY,
    account_id          UUID REFERENCES aitrader.accounts (id),
    session_id          UUID,
    tool_name           TEXT NOT NULL,
    request_payload     JSONB NOT NULL,
    response_payload    JSONB,
    status              TEXT NOT NULL DEFAULT 'success',
    latency_ms          INTEGER,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 行情与绩效快照
CREATE TABLE IF NOT EXISTS aitrader.market_snapshots (
    id                  BIGSERIAL PRIMARY KEY,
    instrument_id       UUID NOT NULL REFERENCES aitrader.instruments (id),
    timeframe           TEXT NOT NULL,
    as_of               TIMESTAMPTZ NOT NULL,
    price               NUMERIC(20, 8),
    ema20               NUMERIC(20, 8),
    ema50               NUMERIC(20, 8),
    macd                NUMERIC(20, 8),
    rsi7                NUMERIC(8, 4),
    rsi14               NUMERIC(8, 4),
    funding_rate        NUMERIC(10, 8),
    open_interest       NUMERIC(24, 4),
    open_interest_avg   NUMERIC(24, 4),
    orderbook_top       JSONB,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (instrument_id, timeframe, as_of)
);

CREATE TABLE IF NOT EXISTS aitrader.performance_snapshots (
    id                      BIGSERIAL PRIMARY KEY,
    account_id              UUID NOT NULL REFERENCES aitrader.accounts (id),
    window                  TEXT NOT NULL,
    sharpe_ratio            NUMERIC(10, 6),
    win_rate                NUMERIC(6, 4),
    average_leverage        NUMERIC(10, 4),
    average_confidence      NUMERIC(5, 2),
    biggest_win             NUMERIC(24, 8),
    biggest_loss            NUMERIC(24, 8),
    hold_ratio_long         NUMERIC(6, 4),
    hold_ratio_short        NUMERIC(6, 4),
    hold_ratio_flat         NUMERIC(6, 4),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, window)
);

-- 策略会话与消息
CREATE TABLE IF NOT EXISTS aitrader.strategy_sessions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id      UUID NOT NULL REFERENCES aitrader.accounts (id),
    title           TEXT,
    status          TEXT NOT NULL DEFAULT 'active',
    metadata        JSONB DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at       TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS aitrader.strategy_messages (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id      UUID NOT NULL REFERENCES aitrader.strategy_sessions (id),
    role            TEXT NOT NULL CHECK (role IN ('assistant', 'user', 'system')),
    content         TEXT NOT NULL,
    summary         TEXT,
    tags            TEXT[] DEFAULT '{}',
    confidence      NUMERIC(5, 2),
    attachments     JSONB DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_strategy_messages_session_time
    ON aitrader.strategy_messages (session_id, created_at DESC);
```

> 完整部署时可将上述 SQL 拆分为迁移脚本（例如 `migrations/0001_init.sql`），确保自动化环境能够 idempotent 地创建/升级数据库结构。
