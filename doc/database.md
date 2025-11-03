# AiTrader 数据存储（OKX USDT 永续）

围绕当前产品的实际场景：所有交易都在 OKX 永续合约上，结算币种固定为 USDT。我们仅保留 MCP 工具和前端所需的最小字段，方便后续实现接口与数据库迁移。

---

## 1. 核心目标

- **简单落地**：只建与 OKX 永续 + USDT 相关的表，暂不抽象多交易所或多币种。
- **快速查询**：满足 MCP `get_account_state` / `get_market_data` / `execute_trade` / `update_exit_plan` / `get_performance_metrics` 的实时读取。
- **可追踪**：记录模型置信度、工具调用与订单的对应关系，便于审计和绩效统计。

---

## 2. 主要表结构

> 以下字段即为代码实现所需，请按表格实现 ORM / SQL 迁移。

### `accounts`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID PK | 内部主键 |
| `external_id` | TEXT UNIQUE | OKX 子账户或模拟账户 ID |
| `mode` | TEXT | `live` / `simulated` |
| `status` | TEXT | `active` / `disabled` 等 |
| `created_at` | TIMESTAMPTZ | 记录创建时间 |

### `balance_snapshots`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGSERIAL PK |  |
| `account_id` | UUID FK → `accounts.id` | |
| `available_usdt` | NUMERIC(24,8) | 可用余额 |
| `locked_usdt` | NUMERIC(24,8) | 冻结余额 |
| `as_of` | TIMESTAMPTZ | 快照时间 |

### `deepseek_credentials`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGSERIAL PK | |
| `api_key` | TEXT | DeepSeek API Key |
| `endpoint` | TEXT | DeepSeek API Endpoint |
| `model` | TEXT | 使用的模型名称 |
| `updated_at` | TIMESTAMPTZ | 最近更新时间 |

### `orders`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID PK | 内部订单 ID |
| `account_id` | UUID FK | |
| `symbol` | TEXT | 如 `BTC-USDT-SWAP` |
| `side` | TEXT | `buy` / `sell` |
| `order_type` | TEXT | `market` / `limit` |
| `price` | NUMERIC(20,8) | 限价单价格（市价可空） |
| `size` | NUMERIC(20,8) | 下单数量（张） |
| `filled_size` | NUMERIC(20,8) | 已成交数量 |
| `status` | TEXT | `open` / `filled` / `canceled` ... |
| `leverage` | NUMERIC(10,2) | 下单时使用的杠杆倍数 |
| `confidence` | NUMERIC(5,2) | 模型置信度（0–100） |
| `tool_call_id` | UUID FK → `mcp_tool_calls.id` | 触发该订单的 MCP 调用 |
| `created_at` | TIMESTAMPTZ | |

### `fills`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID PK | |
| `account_id` | UUID FK | |
| `order_id` | UUID FK | |
| `symbol` | TEXT | |
| `side` | TEXT | |
| `price` | NUMERIC(20,8) | 成交价 |
| `size` | NUMERIC(20,8) | 成交量 |
| `fee_usdt` | NUMERIC(20,8) | 手续费（USDT） |
| `pnl_usdt` | NUMERIC(24,8) | 单笔已实现盈亏（可空） |
| `confidence` | NUMERIC(5,2) | 继承自订单 |
| `timestamp` | TIMESTAMPTZ | 成交时间 |

### `positions_open`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID PK | |
| `account_id` | UUID FK | |
| `symbol` | TEXT | |
| `side` | TEXT | `long` / `short` / `net` |
| `quantity` | NUMERIC(20,8) | 当前仓位 |
| `avg_entry_price` | NUMERIC(20,8) | 均价 |
| `leverage` | NUMERIC(10,2) | 当前杠杆 |
| `margin_usdt` | NUMERIC(24,8) | 占用保证金 |
| `liquidation_price` | NUMERIC(20,8) | 强平价 |
| `unrealized_pnl_usdt` | NUMERIC(24,8) | 未实现盈亏 |
| `exit_plan` | JSONB | 止盈/止损设置（`update_exit_plan` 使用） |
| `opened_at` | TIMESTAMPTZ | 开仓时间 |
| `updated_at` | TIMESTAMPTZ | 最近同步 |

### `positions_closed`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID PK | |
| `account_id` | UUID FK | |
| `symbol` | TEXT | |
| `side` | TEXT | |
| `quantity` | NUMERIC(20,8) | 平仓数量 |
| `entry_price` | NUMERIC(20,8) | 开仓价 |
| `exit_price` | NUMERIC(20,8) | 平仓价 |
| `realized_pnl_usdt` | NUMERIC(24,8) | 已实现盈亏 |
| `holding_minutes` | NUMERIC(14,4) | 持仓时长 |
| `average_confidence` | NUMERIC(5,2) | 该仓位平均置信度 |
| `entry_time` | TIMESTAMPTZ | |
| `exit_time` | TIMESTAMPTZ | |

### `mcp_tool_calls`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | UUID PK | MCP 调用 ID |
| `account_id` | UUID FK | |
| `tool_name` | TEXT | `get_market_data` 等 |
| `request_payload` | JSONB | 入参（脱敏） |
| `response_payload` | JSONB | 返回值 |
| `status` | TEXT | `success` / `failed` |
| `latency_ms` | INTEGER | 耗时 |
| `created_at` | TIMESTAMPTZ | 调用时间 |

### `market_snapshots`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGSERIAL PK | |
| `symbol` | TEXT | 如 `BTC-USDT-SWAP` |
| `timeframe` | TEXT | `1m` / `3m` / `1H` 等 |
| `as_of` | TIMESTAMPTZ | K 线结束时间 |
| `price` | NUMERIC(20,8) | 最新价 |
| `ema20` / `ema50` | NUMERIC(20,8) | EMA 指标 |
| `macd` | NUMERIC(20,8) | MACD 值 |
| `rsi7` / `rsi14` | NUMERIC(8,4) | RSI 指标 |
| `funding_rate` | NUMERIC(10,8) | 最近资金费率 |
| `open_interest` | NUMERIC(24,4) | 最新持仓量 |
| `created_at` | TIMESTAMPTZ | 写入时间 |

### `performance_snapshots`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGSERIAL PK | |
| `account_id` | UUID FK | |
| `window` | TEXT | `daily` / `weekly` / `all_time` 等 |
| `sharpe_ratio` | NUMERIC(10,6) | |
| `win_rate` | NUMERIC(6,4) | |
| `average_leverage` | NUMERIC(10,4) | |
| `average_confidence` | NUMERIC(5,2) | |
| `biggest_win_usdt` | NUMERIC(24,8) | |
| `biggest_loss_usdt` | NUMERIC(24,8) | |
| `hold_ratio_long` / `hold_ratio_short` / `hold_ratio_flat` | NUMERIC(6,4) | 多空占比 |
| `updated_at` | TIMESTAMPTZ | 最近刷新时间 |

---

## 3. 表关系概览

- `accounts` ←→ `balance_snapshots` / `orders` / `fills` / `positions_open` / `positions_closed` / `performance_snapshots`
- `orders` ←→ `fills`（1:N）
- `orders.affects` → 更新 `positions_open`，仓位归零时写入 `positions_closed`
- `orders.tool_call_id` → `mcp_tool_calls.id`
- `market_snapshots` 通过 `symbol + timeframe` 区分数据

### 关系说明

- **下单 (orders)**：记录每次向 OKX 提交的指令，包含方向、数量、杠杆倍数以及模型置信度，并标记是由哪次 MCP 调用触发。
- **成交 (fills)**：当订单被撮合时生成的记录，`fills.order_id` 对应原订单。成交结果会累加到订单的 `filled_size` 并驱动持仓数量变化。
- **当前持仓 (positions_open)**：根据成交数据维护的实时持仓快照。如果一笔成交使某个 symbol/方向的仓位归零，则对应行会被移除。
- **历史持仓 (positions_closed)**：当仓位归零时，将这段持仓的起止时间、均价、盈亏、平均置信度等写入历史表，供绩效统计与审计使用。

---

## 4. 同步流程 (概要)

1. **账户/行情同步**：后台周期性调用 OKX REST，同步余额、持仓、成交，并写入 `market_snapshots`。
2. **交易事件**：收到成交时更新 `orders`、`fills`；仓位清零则写 `positions_closed` 并计算 `average_confidence`。
3. **MCP 调用记录**：每次工具调用写 `mcp_tool_calls`，`execute_trade` 额外关联订单与置信度。
4. **绩效刷新**：定时聚合 `fills`、`positions_closed`，计算指标写入 `performance_snapshots`。

---

## 5. 示例建表 SQL

```sql
CREATE SCHEMA IF NOT EXISTS aitrader;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS aitrader.accounts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    external_id     TEXT NOT NULL UNIQUE,
    mode            TEXT NOT NULL CHECK (mode IN ('live', 'simulated')),
    status          TEXT NOT NULL DEFAULT 'active',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS aitrader.balance_snapshots (
    id              BIGSERIAL PRIMARY KEY,
    account_id      UUID NOT NULL REFERENCES aitrader.accounts (id),
    available_usdt  NUMERIC(24, 8) NOT NULL,
    locked_usdt     NUMERIC(24, 8) NOT NULL DEFAULT 0,
    as_of           TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, as_of)
);

CREATE TABLE IF NOT EXISTS aitrader.deepseek_credentials (
    id          BIGSERIAL PRIMARY KEY,
    api_key     TEXT NOT NULL,
    endpoint    TEXT NOT NULL,
    model       TEXT NOT NULL DEFAULT 'deepseek-chat',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS aitrader.orders (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id      UUID NOT NULL REFERENCES aitrader.accounts (id),
    symbol          TEXT NOT NULL,
    side            TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    order_type      TEXT NOT NULL,
    price           NUMERIC(20, 8),
    size            NUMERIC(20, 8) NOT NULL,
    filled_size     NUMERIC(20, 8) NOT NULL DEFAULT 0,
    status          TEXT NOT NULL,
    leverage        NUMERIC(10, 2),
    confidence      NUMERIC(5, 2),
    tool_call_id    UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS aitrader.fills (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id      UUID NOT NULL REFERENCES aitrader.accounts (id),
    order_id        UUID NOT NULL REFERENCES aitrader.orders (id),
    symbol          TEXT NOT NULL,
    side            TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    price           NUMERIC(20, 8) NOT NULL,
    size            NUMERIC(20, 8) NOT NULL,
    fee_usdt        NUMERIC(20, 8) NOT NULL DEFAULT 0,
    pnl_usdt        NUMERIC(24, 8),
    confidence      NUMERIC(5, 2),
    timestamp       TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS aitrader.positions_open (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id              UUID NOT NULL REFERENCES aitrader.accounts (id),
    symbol                  TEXT NOT NULL,
    side                    TEXT NOT NULL,
    quantity                NUMERIC(20, 8) NOT NULL,
    avg_entry_price         NUMERIC(20, 8),
    leverage                NUMERIC(10, 2),
    margin_usdt             NUMERIC(24, 8),
    liquidation_price       NUMERIC(20, 8),
    unrealized_pnl_usdt     NUMERIC(24, 8),
    exit_plan               JSONB DEFAULT '{}'::jsonb,
    opened_at               TIMESTAMPTZ,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, symbol, side)
);

CREATE TABLE IF NOT EXISTS aitrader.positions_closed (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id              UUID NOT NULL REFERENCES aitrader.accounts (id),
    symbol                  TEXT NOT NULL,
    side                    TEXT NOT NULL,
    quantity                NUMERIC(20, 8) NOT NULL,
    entry_price             NUMERIC(20, 8),
    exit_price              NUMERIC(20, 8),
    realized_pnl_usdt       NUMERIC(24, 8),
    holding_minutes         NUMERIC(14, 4),
    average_confidence      NUMERIC(5, 2),
    entry_time              TIMESTAMPTZ,
    exit_time               TIMESTAMPTZ NOT NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS aitrader.mcp_tool_calls (
    id                  UUID PRIMARY KEY,
    account_id          UUID REFERENCES aitrader.accounts (id),
    tool_name           TEXT NOT NULL,
    request_payload     JSONB NOT NULL,
    response_payload    JSONB,
    status              TEXT NOT NULL DEFAULT 'success',
    latency_ms          INTEGER,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS aitrader.market_snapshots (
    id              BIGSERIAL PRIMARY KEY,
    symbol          TEXT NOT NULL,
    timeframe       TEXT NOT NULL,
    as_of           TIMESTAMPTZ NOT NULL,
    price           NUMERIC(20, 8),
    ema20           NUMERIC(20, 8),
    ema50           NUMERIC(20, 8),
    macd            NUMERIC(20, 8),
    rsi7            NUMERIC(8, 4),
    rsi14           NUMERIC(8, 4),
    funding_rate    NUMERIC(10, 8),
    open_interest   NUMERIC(24, 4),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (symbol, timeframe, as_of)
);

CREATE TABLE IF NOT EXISTS aitrader.performance_snapshots (
    id                      BIGSERIAL PRIMARY KEY,
    account_id              UUID NOT NULL REFERENCES aitrader.accounts (id),
    window                  TEXT NOT NULL,
    sharpe_ratio            NUMERIC(10, 6),
    win_rate                NUMERIC(6, 4),
    average_leverage        NUMERIC(10, 4),
    average_confidence      NUMERIC(5, 2),
    biggest_win_usdt        NUMERIC(24, 8),
    biggest_loss_usdt       NUMERIC(24, 8),
    hold_ratio_long         NUMERIC(6, 4),
    hold_ratio_short        NUMERIC(6, 4),
    hold_ratio_flat         NUMERIC(6, 4),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, window)
);
```

> 若后续需要策略对话表，可基于上一版本文档补充，但系统现阶段并不强制依赖。
