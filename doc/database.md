# AiTrader 数据存储（精简版）

面向当前单账户、OKX 永续合约 + USDT 的工作负载，我们将数据库压缩为两张表即可支撑 MCP 工具和前端查询：`strategies` 记录大模型的最终结论，`orders` 统一存储所有订单（含已完成与执行中）。这份文档列出必需字段与示例 SQL，便于 ORM / 迁移实现。

---

## 1. 设计原则

- **最小可用**：只保留最终策略结论与其衍生的订单，其他衍生快照在需要时可从交易所或日志中复原。
- **实时检索**：前端与 MCP 工具只需读取最近策略结论或订单状态，无需跨表聚合。
- **可追溯**：订单可选关联策略，保留模型置信度、方向、成交进度等关键信息，便于审计。

---

## 2. 表结构

### `strategies`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID PK | 记录唯一 ID |
| `session_id` | TEXT | LLM 会话或策略运行 ID |
| `summary` | TEXT | 大模型最终结论全文 |
| `confidence` | NUMERIC(5,2) | 模型置信度（0–100，可空） |
| `created_at` | TIMESTAMPTZ | 写入时间 |

> 该表只记录最终结论，不再拆分中间推理或建议列表；若需追加原始对话，可在 `summary` 内嵌或后续拓展字段。

### `orders`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID PK | 内部订单 ID |
| `strategy_ids` | UUID[] | 触发该订单的策略 ID 列表（可空数组） |
| `symbol` | TEXT | 如 `BTC-USDT-SWAP` |
| `side` | TEXT | `buy` 代表做多/平空，`sell` 代表做空/平多 |
| `order_type` | TEXT | `market` / `limit` / ... |
| `price` | NUMERIC(20,8) | 限价价格（市价可空） |
| `size` | NUMERIC(20,8) | 申报数量（张） |
| `filled_size` | NUMERIC(20,8) | 已成交数量 |
| `status` | TEXT | `open` / `filled` / `canceled` 等 |
| `leverage` | NUMERIC(10,2) | 下单时的杠杆倍数（可空） |
| `confidence` | NUMERIC(5,2) | 继承策略置信度（可空） |
| `metadata` | JSONB | 扩展信息，如止盈/止损、OKX 原始响应 |
| `created_at` | TIMESTAMPTZ | 下单时间 |
| `closed_at` | TIMESTAMPTZ | 状态变为终态的时间（可空） |

> `orders` 同时覆盖执行中与已完成的合约，借助 `status` 与 `closed_at` 区分阶段；`metadata` 方便保留交易所原始字段或自定义 exit plan。

---

## 3. 查询建议

- 读取“最新策略结论”即按 `created_at DESC` 获取 `strategies`。
- 订单列表按 `symbol` / `status` / 时间范围过滤即可覆盖 MCP `get_account_state`、`execute_trade` 结果展示需求，可同时检索 `strategy_ids` 交集。

---

## 4. 同步 / 写入流程

1. **策略推理完成**：LLM 输出最终结论 → 写入 `strategies`（保存 session、置信度与摘要）。
2. **下单事件**：执行交易时创建 `orders` 行，并在 `strategy_ids` 数组中写入所有关联策略（无需额外关联表）。
3. **成交 / 状态更新**：根据 OKX Webhook 或轮询结果更新 `filled_size`、`status`、`metadata`；当订单进入终态，补写 `closed_at`。

无需额外快照表，余额、持仓可直接通过 OKX API 或实时计算获得。

---

## 5. 示例建表 SQL

```sql
CREATE SCHEMA IF NOT EXISTS aitrader;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS aitrader.strategies (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id      TEXT NOT NULL,
    summary         TEXT NOT NULL,
    confidence      NUMERIC(5, 2),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS aitrader.orders (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_ids    UUID[] NOT NULL DEFAULT ARRAY[]::uuid[],
    symbol          TEXT NOT NULL,
    side            TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    order_type      TEXT NOT NULL,
    price           NUMERIC(20, 8),
    size            NUMERIC(20, 8) NOT NULL,
    filled_size     NUMERIC(20, 8) NOT NULL DEFAULT 0,
    status          TEXT NOT NULL,
    leverage        NUMERIC(10, 2),
    confidence      NUMERIC(5, 2),
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at       TIMESTAMPTZ
);
```
