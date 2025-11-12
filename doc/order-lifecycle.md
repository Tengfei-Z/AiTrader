# 订单生命周期与数据交互设计

## 1. 目标

- 把 OKX 合约/永续订单的整个生命周期拆成可维护的层级（`orders → trades → positions`），
- 以 agent 推送的 `ordId` 事件作为变更源，后端负责消费并用数据库保持当前快照/历史，
- 前端通过后端 API（直接读 DB）获取一致的当前委托与持仓视图。

## 2. 核心表结构

### 2.1 orders（委托 intent）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | UUID | 主键 |
| `ord_id` | TEXT | OKX `ordId`，唯一索引（或存到 JSON metadata） |
| `symbol` | TEXT | 交易对 |
| `side` | TEXT | buy/sell |
| `pos_side` | TEXT | long/short（若有） |
| `order_type` | TEXT | limit/market/stop 等 |
| `size` | NUMERIC | 请求数量 |
| `price` | NUMERIC | 限价单价格（可空） |
| `leverage` | NUMERIC | 杠杆（可空） |
| `status` | TEXT | 最后接收状态（新/已撤/完成） |
| `last_event_at` | TIMESTAMPTZ | 最后一次 agent 回调时间 |
| `metadata` | JSONB | 原始 payload（agent 事件） |

**说明**：`orders` 表存的是 OKX 的委托（无论是开仓或平仓），不在此计算 fill；agent 每次 `order_update` 以 `ord_id` 为 key  upsert 这条记录，保持最新状态与 metadata，主要供策略/审计与对账使用，即使同一订单号经历平仓也会更新而非新增行。

### 2.2 trades（成交/回报）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | UUID | 主键 |
| `ord_id` | TEXT | 关联 OKX `ordId` |
| `trade_id` | TEXT | OKX 返回的 fill 标识 |
| `symbol` | TEXT | 交易对 |
| `side` | TEXT | buy/sell |
| `filled_size` | NUMERIC | 本条成交量 |
| `fill_price` | NUMERIC | 成交均价 |
| `fee` | NUMERIC | 手续费 |
| `realized_pnl` | NUMERIC | 这笔 fill 产生的盈亏 |
| `ts` | TIMESTAMPTZ | 成交时间 |
| `metadata` | JSONB | okx/agent 返回的完整 payload |

**说明**：`trades` 是成交真相，用于衡量真实成本与盈亏。收到 agent event 时若确认是 fill（`filled_size`>0），就 insert 一条，以防 fill 推送多次。

### 2.3 positions（持仓快照）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `symbol` | TEXT | 主键（或 `symbol+pos_side`） |
| `side` | TEXT | long/short/net |
| `size` | NUMERIC | 当前净持仓 |
| `avg_price` | NUMERIC | 加权买入价 |
| `unrealized_pnl` | NUMERIC | 标记盈亏 |
| `margin` | NUMERIC | 占用保证金 |
| `last_trade_at` | TIMESTAMPTZ | 最近一次 fill 时间 |
| `closed_at` | TIMESTAMPTZ | 置空即表示当前仍持仓 |
| `metadata` | JSONB | 其他信息（如 `okx_position_id`） |
| `action_kind` | TEXT | `exit`／`forced`／空：区分主动 vs 被动平仓 |
| `entry_ord_id` | TEXT | 对应建仓 ordId（若持仓起始来自某次委托） |
| `exit_ord_id` | TEXT | 当 `closed_at` 有值时记录对应的平仓 ordId，便于回溯 |

**说明**：positions 由 trades 聚合而成，可通过物化视图/触发器异步更新；也可以在 agent event 里直接计算（例如 `size += filled_change`），只需每次 `trades` insert 后更新一次 positions `size`/`avg_price`/`unrealized_pnl`。

## 3. 事件交互流程

1. **agent 下单产生 ordId**：agent 调用 OKX API 后拿到 ordId，仅将这个标识（和必要 metadata）通过 websocket 推给后端 `order_update` 事件；后续状态追踪与差异处理由 Rust 后端负责。
2. **后端处理事件**：
   - 解析 payload（`inst_id`、`side`、`state`、`filledSz`、`reduceOnly` 等）。
   - `orders` 表 upsert（ordId → status/metadata/last_event_at，若不存在则 insert）。
   - 若存在成交（`filled_size>0`），在 `trades` 表插入一条，记录 fee/price。
   - 依据 `trades` 聚合或直接调整 `positions` 的 `size`、`avg_price`、`unrealized_pnl`；若持仓清零且状态 terminal，可填 `closed_at`，前端通过这个字段区分历史/当前。
   - 每次 `positions` 变更时维护 `entry_ord_id`（首次建仓的 ordId）与 `exit_ord_id`（当 `closed_at` 被填时写入当前 ordId），确保每个历史仓位都能对齐到一条平仓事件；`action_kind`（`exit`/`forced`）则标识是 agent 主动还是被动平仓。
   - **定时器启动**：若 agent 只发一次，Rust 需开启定时轮询（例如 every minute）调用 OKX `/orders`/`/fills`/`/positions`，用最新数据补全 `trades` 及 `positions` 状态，确保任意 `inst_id` 即使没有后续 websocket 也有更新。
3. **前端读取**：
   - 持仓页面显示 `positions`（净仓对齐），历史仓位通过 `closed_at IS NOT NULL` 或 `closed_at` 非空的视图。
   - API `/api/positions` 与 `/api/positions/history` 直接查询本地 `positions` 表（当前/历史），只返回 agent 处理过的 ordId；前端不再连 OKX `/positions`。
   - 若需要 fill 详情/盈亏分解，可直接查询 `trades`。

## 4. 建/平仓关联与仓位定位

- **定位仓位**：agent 事件里会附带 `instId`/`side`/`posSide`，Rust 可以用这些字段定位到具体 `positions` 行（`inst_id`+方向），再据 `filled_size`、`size` 等调整该行 `qty` 与 `avg_price`。
- **建/平关联**：初次建仓时把 `positions.entry_ord_id` 填为那笔 ordId；一旦该仓位被 agent 平掉，就在同一行写入 `exit_ord_id=当前 ordId`、`closed_at=now()`、`action_kind="exit"`，前端即可通过这对 id 关联建平事件。
- **被动平仓**：若周期同步发现某个 `inst_id+pos_side` 不再出现在 OKX `/positions`（该接口只返回活跃仓位），就把 `action_kind="forced"`、`size=0`、`closed_at` 补齐，但仅作用于已有 `entry_ord_id` 的行，避免把 agent 未知的旧订单写入。
- **前端呈现**：只要画面显示 `positions`，通过 `closed_at` + `action_kind` 就能分别识别当前持仓、主动平仓与被动平仓，无需额外展示 `orders`。

## 5. 周期轮询补全
   - 由于 agent 可能只在下单时推送一次 ordId，Rust 后端需要周期性（例如每分钟/每个策略运行后）调用 OKX 的 `/fills` 和 `/positions`，用命令行接口补齐 `trades` 与 `positions` 的缺口。
   - 这个定时任务会拉取 OKX `/positions`（只返回活跃仓位），按 `inst_id+pos_side` 与本地 `positions` 对比：对新出现的更新 snapshot、对于不再返回的（且已有 `entry_ord_id`）标记 `forced` 平仓并写 `closed_at`。
   - 脚本还可以作为对账工具，帮助定位 agent 丢包或 OKX 状态延迟。

## 6. Agent 与后端协同建议

- Agent 负责把 “事件 + metadata” 及时推送，并且后端仅记录 agent 送来的 `ordId` —— 周期补全不引入新的订单号，避免写入旧订单。后端再次同步 OKX 时也只会标记还在 `positions` 表里、曾由 agent 处理过的行。
- 若 agent 事件缺字段（例如只剩 `ordId`），后端可加定时任务 `GET /orders/<ordId>` 或 `order_history` 来补全，优先通过 agent 补充 metadata。
- 为防重复事件，可在 `trades` 上加唯一索引 `ord_id + trade_id`，`orders` 使用 `ord_id` 唯一约束，方便 `upsert`。
## 7. 关键优化（最重要的 8 点）

1. **统一主标识：`inst_id`**  
   替代 `symbol`，所有表都用 OKX 的合约名（如 `BTC-USDT-SWAP`）作为主标识，避免「交易对/永续/交割」混淆。

2. **`ordId` 升格为列并建立唯一索引**  
   - `orders.ord_id TEXT UNIQUE NOT NULL`（OKX 下发）。  
   - 后续查询直接 `WHERE ord_id = $1`，不再依赖 `metadata->>'ordId'`。

3. **数值精度放宽**  
   统一使用 `NUMERIC(36,18)` 或不限精度的 `NUMERIC`，兼容杠杆合约与小众币种数据，避免 `20,8` 溢出。

4. **记录账户模式与持仓侧**  
   `orders.td_mode`（cross/isolated）、`positions.pos_side`（long/short/net）必须持久化，便于区分全仓与逐仓、多空分持。

5. **`trades` 去重唯一键**  
   建 `UNIQUE(ord_id, trade_id)`，若有些 fill 没 `trade_id`，可增加 `fingerprint = hash(ord_id || ts || px || filled_size)` 作为容错，防止重复写入。

6. **`positions` 只存“状态”，真相在 `trades`**  
   通过 `trades` 增量更新 `positions` 的 `qty`/`avg_entry_px`/`last_trade_at`/`closed_at`，未实现只保留 `mark_px`/`last_mark_px`，已实现盈亏从 `trades.realized_pnl` 聚合（加上 funding）。

7. **新增资金费表**  
   `funding_payments(inst_id, side, rate, payment, ccy, paid_at)` 专门记录永续资金费，避免把资金成本混进 trades，更便于策略回测。

8. **事件幂等与对账机制**  
   每次写表必须带 `ord_id`/`trade_id` 唯一键；定期调用 OKX 官方 `/fills`、`/positions` 做一次对账，记录差异并同步补录。
