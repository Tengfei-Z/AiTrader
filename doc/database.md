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

4. **余额变化记录**：在监听 OKX 账户接口（或 webhook）时，只要最新 `valuation_usdt` 与上一次写入的快照不同，就在 `balances` 表新增一条记录。记录频率可控，必要时通过预估策略行为精细写入，避免无意义的重复条目。

5. **定时同步**：后端启动后会每 5 秒主动去 OKX 拉取余额并尝试写入快照，使得即便当前没有前端访问 ` /account/balances`，数据库里仍有最新估值。UI 仍可通过接口触发即时刷新，但历史曲线与“当前金额”将依赖持久化的快照数据。

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

CREATE TABLE IF NOT EXISTS aitrader.balances (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    asset           TEXT NOT NULL DEFAULT 'USDT',
    available       NUMERIC(20, 8) NOT NULL,
    locked          NUMERIC(20, 8) NOT NULL,
    valuation       NUMERIC(20, 8) NOT NULL,
    source          TEXT NOT NULL DEFAULT 'okx',
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

## 5. 余额快照接口

- `GET /account/balances/snapshots?limit=100`：按 `recorded_at DESC` 返回最近 `limit` 条 `balances` 记录，给前端图表提供历史数据。
- `GET /account/balances/latest`：返回最新一条 snapshot（或代理实时 OKX 数据），用于 “当前金额” 面板与收益曲线的实时对齐。

两者的数据可由 `balances` 表直接读取，也可以在没有记录时 fallback 到即时拉取的 OKX 余额。前端在展示时：新记录只有当 `valuation` 或 `available` 发生变化才会写入，从而防止频繁更新同时能保证图表在“有变化时”自动增长。
