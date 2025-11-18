# AiTrader 数据存储

后端使用 PostgreSQL（默认 schema：`aitrader`）保存 Agent 推理结果、订单、持仓与资金相关数据。`backend/src/db.rs` 在启动时自动创建所需表、索引与 `pgcrypto` 扩展。本文档复述当前代码中的真实结构与写入流程，作为 ORM、迁移或数据分析的单一事实来源。

---

## 1. schema 约定

- **数据库**：PostgreSQL，URL / schema 由 `config/config.yaml` 或环境变量 `DATABASE_URL`、`DATABASE_SCHEMA` 控制，默认 schema 为 `aitrader`。
- **扩展**：`pgcrypto` 必须可用（用于 `gen_random_uuid()`）。`init_database` 在缺失时尝试 `CREATE EXTENSION IF NOT EXISTS pgcrypto`。
- **主键**：所有表均采用 `UUID PRIMARY KEY DEFAULT gen_random_uuid()`；另有业务侧 `ord_id`（OKX 原始订单号）作为 `orders` 的唯一键。
- **时间字段**：统一使用 `TIMESTAMPTZ`，由数据库 `DEFAULT now()` 记录。
- **数值字段**：金额/数量以 `NUMERIC`（或 `NUMERIC(20,8)`）存储，读写时在 `db.rs` 中通过 `double precision` 转换。

---

## 2. 表总览

| 表名 | 功能 | 主要消费方 |
|------|------|------------|
| `strategies` | 保存最近的 AI 策略摘要 | `/api/model/strategy-chat` |
| `orders` | Agent 订单与其同步状态 | order_sync、后端诊断 |
| `trades` | 订单成交明细（fills） | order_sync、盈亏计算 |
| `positions` | 当前/历史持仓快照 | `/api/account/positions*` |
| `balances` | 账户余额快照（USDT） | `/api/account/balances/*` |
| `initial_equities` | 初始资金配置 | `/api/account/initial-equity` |

---

## 3. 表结构

### 3.1 `strategies`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 主键，由数据库生成 |
| `summary` | TEXT | Agent 输出的策略摘要（包含推理/结论） |
| `created_at` | TIMESTAMPTZ | 写入时间，`DEFAULT now()` |

> 注意：当前实现未保存 `session_id`，`insert_strategy_message` 仅写入 `summary`。

### 3.2 `orders`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 内部主键 |
| `strategy_ids` | UUID[] | 与订单关联的策略 ID 列表（默认空数组） |
| `ord_id` | TEXT | OKX `ordId`，唯一 |
| `inst_id` | TEXT | 交易对（如 `BTC-USDT-SWAP`） |
| `td_mode` | TEXT | 逐仓/全仓等模式 |
| `pos_side` | TEXT | `long` / `short` / `net` / `cross` 等 |
| `side` | TEXT | `buy` / `sell`，有 CHECK 约束 |
| `order_type` | TEXT | `market` / `limit` 等 |
| `price` | NUMERIC | 报价；市价单可为空 |
| `size` | NUMERIC | 下单数量 |
| `filled_size` | NUMERIC | 已成交数量，默认 0 |
| `status` | TEXT | OKX 状态原文 |
| `leverage` | NUMERIC | 杠杆倍数 |
| `action_kind` | TEXT | agent / manual / forced 等自定义标记 |
| `entry_ord_id` / `exit_ord_id` | TEXT | 与仓位建立/平仓关联的其他订单号 |
| `last_event_at` | TIMESTAMPTZ | 最近更新事件时间戳 |
| `metadata` | JSONB | OKX 原始响应或扩展字段（默认 `{}`） |
| `created_at` | TIMESTAMPTZ | 记录创建时间 |
| `closed_at` | TIMESTAMPTZ | 终态结束时间 |

### 3.3 `trades`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 主键 |
| `ord_id` | TEXT | 对应 `orders.ord_id` |
| `trade_id` | TEXT | OKX 成交 ID（可空） |
| `fingerprint` | TEXT | 由 `trade_record_from_fill` 生成的幂等键 |
| `inst_id`, `td_mode`, `pos_side`, `side` | TEXT | 与订单保持一致；`side` 仍受 `buy/sell` 约束 |
| `filled_size` | NUMERIC | 本次成交数量 |
| `fill_price` | NUMERIC | 成交价 |
| `fee` | NUMERIC | 交易所手续费 |
| `realized_pnl` | NUMERIC | 已实现盈亏（若可得） |
| `ts` | TIMESTAMPTZ | 成交时间 |
| `metadata` | JSONB | 透传 OKX fill 字段 |

> `trades` 通过唯一索引 `ON (ord_id, trade_id)` 去重；`insert_trade_record` 使用 `ON CONFLICT DO NOTHING`。

### 3.4 `positions`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 主键 |
| `inst_id` | TEXT | 交易对 |
| `pos_side` | TEXT | 多/空/净方向 |
| `td_mode` | TEXT | 仓位模式 |
| `side` | TEXT | `long` / `short` / `net`（CHECK 约束） |
| `size` | NUMERIC | 仓位数量（默认 0） |
| `avg_price` | NUMERIC | 开仓均价 |
| `mark_px` | NUMERIC | 标记价格 |
| `margin` | NUMERIC | 占用保证金 |
| `unrealized_pnl` | NUMERIC | 未实现盈亏 |
| `last_trade_at` | TIMESTAMPTZ | 最近一次成交时间 |
| `closed_at` | TIMESTAMPTZ | 平仓时间 |
| `action_kind` | TEXT | `agent` / `manual` / `forced` 等 |
| `entry_ord_id` / `exit_ord_id` | TEXT | 链接 `orders.ord_id`，外键 `ON DELETE SET NULL` |
| `metadata` | JSONB | 自定义字段 |
| `updated_at` | TIMESTAMPTZ | 最后更新（`upsert_position_snapshot` 中刷新） |
| `snapshot_id` | BIGINT | `IDENTITY` 序列，用于时间序列回放 |

> 约束：`(inst_id, pos_side)` 在 `closed_at IS NULL` 时唯一，用作当前持仓键；另有唯一索引在 `snapshot_id` 上防止重复。

### 3.5 `balances`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 主键 |
| `asset` | TEXT | 默认 `USDT` |
| `available` | NUMERIC(20,8) | 可用余额 |
| `locked` | NUMERIC(20,8) | 冻结余额 |
| `valuation` | NUMERIC(20,8) | 总权益 |
| `source` | TEXT | 默认 `okx`，可用于区分真实/模拟账户 |
| `recorded_at` | TIMESTAMPTZ | 快照时间 |

### 3.6 `initial_equities`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID | 主键 |
| `amount` | NUMERIC(20,8) | 初始资金数额 |
| `recorded_at` | TIMESTAMPTZ | 写入时间 |

---

## 4. 数据流与写入流程

1. **策略消息**：Agent 结束一次分析后调用 `insert_strategy_message` 写入 `strategies`；前端 `GET /api/model/strategy-chat` 通过 `fetch_strategy_messages` 读取最新 15 条记录。
2. **订单事件**：`order_sync::process_agent_order_event` 解析 OKX 历史记录并构造 `AgentOrderEvent`。`db::upsert_agent_order` 将 `ord_id` 对应的行写入 `orders`：若已存在则更新 `status`、`filled_size`、`metadata`、`leverage`、`td_mode` 等字段，并在终态时设置 `closed_at`。
3. **成交记录**：`order_sync` 同步每个 `fill` 到 `trades`，用于盈亏追踪与幂等去重。
4. **持仓快照**：`order_sync::run_periodic_position_sync` 以及 Agent 事件在 `db::upsert_position_snapshot` 中刷新 `positions`。若 OKX 已不再返回某个持仓，则调用 `mark_position_forced_exit` 将其标记为 `closed` 并记录 `action_kind = 'forced'`。
5. **余额快照**：`routes::account::run_balance_snapshot_loop` 每 30 分钟（默认）读取 OKX 余额并调用 `insert_balance_snapshot` 写入 `balances`，当前接口和历史曲线都从此表读取。写入逻辑包含绝对与相对变化阈值，避免噪声。
6. **初始资金**：`/api/account/initial-equity` 的 `POST` 经 `insert_initial_equity` 覆盖表内容，仅保存最新一条。`GET` 会优先使用环境变量 `INITIAL_EQUITY`（若配置）并回写数据库。

---

## 5. 建表 SQL（与 `db.rs` 保持一致）

```sql
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE SCHEMA IF NOT EXISTS aitrader;

CREATE TABLE IF NOT EXISTS aitrader.strategies (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    summary    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS aitrader.orders (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_ids  UUID[] NOT NULL DEFAULT ARRAY[]::uuid[],
    ord_id        TEXT NOT NULL UNIQUE,
    inst_id       TEXT NOT NULL,
    td_mode       TEXT,
    pos_side      TEXT,
    side          TEXT NOT NULL CHECK (side IN ('buy','sell')),
    order_type    TEXT NOT NULL,
    price         NUMERIC,
    size          NUMERIC NOT NULL,
    filled_size   NUMERIC NOT NULL DEFAULT 0,
    status        TEXT NOT NULL,
    leverage      NUMERIC,
    action_kind   TEXT,
    entry_ord_id  TEXT,
    exit_ord_id   TEXT,
    last_event_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at     TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS aitrader.trades (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ord_id        TEXT NOT NULL,
    trade_id      TEXT,
    fingerprint   TEXT,
    inst_id       TEXT NOT NULL,
    td_mode       TEXT,
    pos_side      TEXT,
    side          TEXT NOT NULL CHECK (side IN ('buy','sell')),
    filled_size   NUMERIC NOT NULL,
    fill_price    NUMERIC,
    fee           NUMERIC,
    realized_pnl  NUMERIC,
    ts            TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata      JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE UNIQUE INDEX IF NOT EXISTS aitrader_trades_ord_id_trade_id_uindex ON aitrader.trades (ord_id, trade_id);

CREATE TABLE IF NOT EXISTS aitrader.positions (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    inst_id        TEXT NOT NULL,
    pos_side       TEXT,
    td_mode        TEXT,
    side           TEXT NOT NULL CHECK (side IN ('long','short','net')),
    size           NUMERIC NOT NULL DEFAULT 0,
    avg_price      NUMERIC,
    mark_px        NUMERIC,
    margin         NUMERIC,
    unrealized_pnl NUMERIC,
    last_trade_at  TIMESTAMPTZ,
    closed_at      TIMESTAMPTZ,
    action_kind    TEXT,
    entry_ord_id   TEXT,
    exit_ord_id    TEXT,
    metadata       JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    snapshot_id    BIGINT GENERATED BY DEFAULT AS IDENTITY
);
ALTER TABLE aitrader.positions
    ADD CONSTRAINT aitrader_positions_entry_ord_fk FOREIGN KEY (entry_ord_id) REFERENCES aitrader.orders (ord_id) ON DELETE SET NULL;
ALTER TABLE aitrader.positions
    ADD CONSTRAINT aitrader_positions_exit_ord_fk FOREIGN KEY (exit_ord_id) REFERENCES aitrader.orders (ord_id) ON DELETE SET NULL;
CREATE UNIQUE INDEX IF NOT EXISTS aitrader_positions_inst_id_pos_side_open_idx ON aitrader.positions (inst_id, pos_side) WHERE closed_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS aitrader_positions_snapshot_id_uindex ON aitrader.positions (snapshot_id);

CREATE TABLE IF NOT EXISTS aitrader.balances (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    asset       TEXT NOT NULL DEFAULT 'USDT',
    available   NUMERIC(20, 8) NOT NULL,
    locked      NUMERIC(20, 8) NOT NULL,
    valuation   NUMERIC(20, 8) NOT NULL,
    source      TEXT NOT NULL DEFAULT 'okx',
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS aitrader.initial_equities (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    amount      NUMERIC(20, 8) NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

---

## 6. 查询与消费模式

- **策略历史**：`fetch_strategy_messages(limit)` 以 `created_at DESC` 读取 `strategies`，供 UI 展示最近摘要。
- **订单状态**：`upsert_agent_order` / `process_agent_order_event` 通过 `ord_id` 更新 `orders`，诊断工具可以按 `inst_id`、`status`、`action_kind` 过滤。
- **成交/盈亏**：`trades` 支撑后续盈亏或审计需求，按 `ord_id` / `ts` 查询。
- **持仓视图**：`fetch_position_snapshots(include_history, symbol, limit)` 根据 `closed_at` 是否为空返回当前或历史仓位，字段与 `/api/account/positions*` 完全一致。
- **余额曲线**：`fetch_balance_snapshots`、`fetch_latest_balance_snapshot` 为 `/api/account/balances/*` 提供数据；当数据库为空时接口回退到实时 OKX 余额。
- **初始资金**：`fetch_initial_equity` 返回最新一条记录，若无记录则通过配置回填并写回数据库。

如需新增字段或表，请同步更新 `backend/src/db.rs` 与本文档，保持双向一致。
