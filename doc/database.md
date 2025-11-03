# AiTrader 数据存储设计（初稿）

本文档汇总当前交易控制台需要持久化的数据、数据库选型以及核心表结构，后续版本可按上线阶段持续补充。目标读者为后端与数据工程同学，便于实现统一的数据访问层以及为前端 API 提供稳定的契约。

---

## 1. 设计目标

- **集中化**：将账户、持仓、成交、策略对话等数据统一落地，既能支持前端看板也便于统计分析。
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
2. **交易执行域**：订单、成交、当前持仓、历史持仓。
3. **策略对话域**：大模型对话、执行建议、人工反馈。
4. **行情/元数据域**：标的基本信息、快照或聚合行情（需要时扩展）。
5. **日志追踪域（预留）**：策略执行流水、风控告警、外部系统回执。

---

## 4. PostgreSQL 表设计

> 以下示例字段仅展示核心列，实际建表时可根据业务落地更多属性（如创建人、更新时间、软删除等）。所有表建议增加 `created_at TIMESTAMPTZ DEFAULT now()` 与 `updated_at TIMESTAMPTZ` 并通过触发器维护。

### 4.1 账户与资产

#### `accounts`
| 字段              | 类型            | 说明                                  |
|-------------------|-----------------|---------------------------------------|
| `id`              | UUID PK         | 内部主键                              |
| `external_id`     | TEXT UNIQUE     | 交易所账号或模拟账号标识             |
| `mode`            | TEXT            | `live` / `simulated`                  |
| `status`          | TEXT            | `active` / `disabled` 等              |
| `metadata`        | JSONB           | 账户别名、描述等                      |

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

> **索引建议**：`(account_id, asset, as_of DESC)`；可按照 `as_of` 做时间分区。

### 4.2 订单与成交

#### `orders`
| 字段           | 类型            | 说明                                             |
|----------------|-----------------|--------------------------------------------------|
| `id`           | UUID PK         | 内部订单 ID（便于跨交易所）                     |
| `account_id`   | UUID FK         | 对应账户                                        |
| `exchange_order_id` | TEXT      | 交易所订单号                                    |
| `symbol`       | TEXT            | 交易对                                          |
| `side`         | TEXT            | `buy`/`sell`                                    |
| `order_type`   | TEXT            | `limit`/`market` 等                              |
| `price`        | NUMERIC         | 限价单价格，可空                                 |
| `size`         | NUMERIC         | 下单数量                                         |
| `filled_size`  | NUMERIC         | 已成交数量                                       |
| `status`       | TEXT            | `open`/`partially_filled`/`filled`/`canceled`…  |
| `time_in_force`| TEXT            | 可选                                           |
| `created_at`   | TIMESTAMPTZ     | 下单时间                                         |
| `updated_at`   | TIMESTAMPTZ     | 状态更新时间                                     |

#### `fills`
| 字段           | 类型            | 说明                                    |
|----------------|-----------------|-----------------------------------------|
| `id`           | UUID PK         | 内部成交 ID                             |
| `account_id`   | UUID FK         |                                         |
| `order_id`     | UUID FK         | 引用 `orders`                           |
| `exchange_fill_id` | TEXT       | 交易所成交编号                          |
| `symbol`       | TEXT            |                                         |
| `side`         | TEXT            |                                         |
| `price`        | NUMERIC         | 成交价                                  |
| `size`         | NUMERIC         | 成交量                                  |
| `fee`          | NUMERIC         | 手续费                                  |
| `pnl`          | NUMERIC         | 单笔已实现盈亏（若交易所返回）          |
| `timestamp`    | TIMESTAMPTZ     | 成交时间                                |
| `raw_payload`  | JSONB           | 交易所回执原文（便于审计）              |

> **索引建议**：`(account_id, symbol, timestamp DESC)`；可加 `order_id` 索引用于订单视图。

### 4.3 持仓数据

#### `positions_open`
| 字段            | 类型        | 说明                                      |
|-----------------|-------------|-------------------------------------------|
| `id`            | UUID PK     |                                           |
| `account_id`    | UUID FK     |                                           |
| `symbol`        | TEXT        |                                           |
| `side`          | TEXT        | `long` / `short` / `net`                  |
| `quantity`      | NUMERIC     | 当前持仓张数                              |
| `avg_entry_price` | NUMERIC   | 加权成本价                                |
| `leverage`      | NUMERIC     | 当前杠杆                                  |
| `margin`        | NUMERIC     | 占用保证金                                |
| `liquidation_price` | NUMERIC | 强平价（若有）                            |
| `unrealized_pnl` | NUMERIC    | 未实现盈亏                                |
| `opened_at`     | TIMESTAMPTZ | 建仓时间                                  |
| `updated_at`    | TIMESTAMPTZ | 最近一次同步                              |

#### `positions_closed`
用于填充前端“历史持仓”，也驱动盈亏统计。

| 字段            | 类型        | 说明                                         |
|-----------------|-------------|----------------------------------------------|
| `id`            | UUID PK     |                                              |
| `account_id`    | UUID FK     |                                              |
| `symbol`        | TEXT        |                                              |
| `side`          | TEXT        |                                              |
| `quantity`      | NUMERIC     | 平仓数量                                    |
| `entry_price`   | NUMERIC     | 开仓均价                                    |
| `exit_price`    | NUMERIC     | 平仓均价                                    |
| `leverage`      | NUMERIC     | 开仓时杠杆                                  |
| `margin`        | NUMERIC     | 保证金                                      |
| `realized_pnl`  | NUMERIC     | 已实现盈亏                                  |
| `entry_time`    | TIMESTAMPTZ | 入场时间                                    |
| `exit_time`     | TIMESTAMPTZ | 离场时间                                    |
| `source`        | TEXT        | `exchange` / `simulator` / `manual` 等       |
| `raw_payload`   | JSONB       | 交易所返回的原始记录                         |

> 平仓检测策略：接受 `fills` 流后更新 `positions_open`，当仓位归零时写入 `positions_closed`。

### 4.4 策略对话与操作记录

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
| `confidence`  | NUMERIC     | 模型置信度，可选                             |
| `created_at`  | TIMESTAMPTZ | 消息时间                                     |
| `attachments` | JSONB       | 关联的订单草稿、行情快照等                   |

#### `strategy_actions`（可选）
记录模型建议与实际执行的映射，以便审计。

| 字段          | 类型        | 说明                                                |
|---------------|-------------|-----------------------------------------------------|
| `id`          | UUID PK     |                                                     |
| `message_id`  | UUID FK     | 来源消息                                            |
| `order_id`    | UUID FK     | 关联成功下发的订单                                 |
| `status`      | TEXT        | `pending` / `executed` / `rejected`                |
| `reason`      | TEXT        | 失败或拒绝原因                                     |
| `created_at`  | TIMESTAMPTZ |                                                     |

### 4.5 行情与快照（可选）

若需要保存行情，可新增：

- `market_ticks`：分钟或秒级聚合，字段包含 `symbol`, `last_price`, `volume`, `as_of`。
- `equity_snapshots`：存储每日或每小时的权益曲线点，减少前端实时计算压力。字段：`account_id`, `equity`, `cash`, `net_position_value`, `as_of`。

---

## 5. 表关系（ER 简述）

- `accounts` ←→ `balance_snapshots`（1:N）
- `accounts` ←→ `orders` ←→ `fills`
- `accounts` ←→ `positions_open` / `positions_closed`
- `accounts` ←→ `strategy_sessions` ←→ `strategy_messages`
- `strategy_messages` ←→ `strategy_actions`（可选 1:1）

所有交易域表均通过 `account_id` 绑定，方便区分实盘与模拟账户。若未来存在多交易所，可在对应表补充 `exchange` 字段或拆分 schema。

---

## 6. 同步与写入流程

1. **行情/账户拉取**：OKX Connector 定期调用 REST / WebSocket，同步余额、活跃持仓和成交。
2. **事件处理**：数据进入处理器（Rust Service），按照交易事件更新 `orders`、`fills`、`positions_open`，并在仓位归零时写 `positions_closed`。
3. **策略对话**：大模型生成回复后，将消息写入 `strategy_messages`，若产生指令则追加 `strategy_actions`。前端用户交互亦写同表。
4. **前端查询**：控制台通过 API 读取 `positions_open/closed`、`strategy_messages` 等表，支持分页和过滤。
5. **快照归档**：`balance_snapshots` / `equity_snapshots` 可按日批量归档或迁移至对象存储。

---

## 7. 索引与性能建议

- 常规索引：对 `account_id`、`symbol`、`timestamp/created_at` 等常用查询字段建立联合索引。
- 分区策略：大体量表（如 `fills`, `strategy_messages`）按月份或季度分区，避免单表膨胀。
- JSONB 字段需结合 GIN 索引按需建立（如 `raw_payload` 内部查询）。

---

## 8. 数据保留与清理

| 数据类型          | 建议保留周期       | 处理方式                         |
|-------------------|--------------------|----------------------------------|
| 订单/成交/持仓    | 长期保留           | 只做分区与归档                   |
| 策略对话          | ≥ 1 年（便于审计） | 可按会话归档至对象存储           |
| 行情快照          | 30~90 天热存储     | 冷数据落 Kafka / S3              |
| 余额快照          | ≥ 180 天           | 过期数据按月归档或聚合           |

---

## 9. 风险与注意事项

1. **策略对话敏感信息**：需做好访问控制，必要时加密或脱敏（例如隐藏用户的手动指令原文）。
2. **多账户/多交易所兼容**：提前保留 `exchange` 或 `strategy_id` 字段，避免未来扩容导致迁移难度大。
3. **一致性**：面对交易所接口异常时，需记录补偿任务，将缺失的订单/成交补齐。
4. **Schema 变更**：建议采用迁移工具（如 `sqlx migrate`/`diesel`），同步更新该文档。

---

## 10. 后续工作

- 根据此文档编写正式 SQL 建表脚本。
- 在 API 层加入分页、过滤参数映射对应表结构。
- 将模型会话的读取/写入接口纳入后端服务，并替换前端 mock 数据。
- 若引入 Kafka，规划事件溯源与回放机制，支持更精细的策略回测。

欢迎在需求发生变化时补充本文件，确保前后端与数据团队对齐。

