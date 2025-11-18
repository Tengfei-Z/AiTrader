# AiTrader 前后端接口说明

本文档描述 Rust/Axum 后端向前端暴露的 HTTP 契约。API 默认返回 `application/json`，响应结构统一为：

```json
{
  "success": true,
  "data": { ... },  // 或 null
  "error": null     // 失败时为错误描述
}
```

当 `success = false` 时，`data` 恒为 `null`，`error` 含可读的错误信息，可选地附带 `code` 字段表示错误类型（见第 5 节）。

> **路径约定**：所有接口可通过 `/api/<path>` 访问；由于后端同时将路由挂载在根路径，也允许直接访问 `/market/...`、`/account/...`，但推荐统一带 `/api` 前缀，方便 Nginx 鉴权与限流。

---

## 1. 行情接口

### 1.1 获取单个交易对最新行情
- **Method**: `GET`
- **Path**: `/api/market/ticker`
- **Query**:
  - `symbol` (string, required)：交易对 ID，如 `BTC-USDT`
- **说明**：直接透传 OKX `Ticker` 数据，K 线周期固定为 `3m`。

`data` 字段说明：

| 字段        | 类型    | 说明                                   |
|-------------|---------|----------------------------------------|
| `symbol`    | string  | 交易对                                 |
| `bar`       | string  | OKX `bar` 周期                         |
| `last`      | string  | 最新成交价                             |
| `open24h`   | string? | 24 小时开盘价                          |
| `bidPx`     | string? | 买一价                                 |
| `askPx`     | string? | 卖一价                                 |
| `high24h`   | string? | 24 小时最高价                          |
| `low24h`    | string? | 24 小时最低价                          |
| `vol24h`    | string? | 24 小时成交量（张）                    |
| `volCcy24h` | string? | 24 小时成交量（币）                    |
| `timestamp` | string  | 毫秒级时间戳                           |

---

## 2. 账户数据接口

### 2.1 实时账户余额
- **Method**: `GET`
- **Path**: `/api/account/balances`
- **说明**：实时调用 OKX 账户余额接口，仅返回 USDT 资产。

| 字段             | 类型   | 说明                   |
|------------------|--------|------------------------|
| `asset`          | string | 资产类型，固定 `USDT` |
| `available`      | string | 可用余额，6 位小数     |
| `locked`         | string | 冻结余额，6 位小数     |
| `valuation_usdt` | string | 总权益（USDT），6 位小数 |

```json
{
  "success": true,
  "data": {
    "asset": "USDT",
    "available": "15432.500000",
    "locked": "1500.000000",
    "valuation_usdt": "16932.500000"
  },
  "error": null
}
```

### 2.2 余额快照列表
- **Method**: `GET`
- **Path**: `/api/account/balances/snapshots`
- **Query**:
  - `limit` (integer, optional，默认 100，最大 1000)
  - `asset` (string, optional，默认 `USDT`)
  - `after` (string, optional)：RFC3339 时间戳，仅返回晚于该时间的记录
- **说明**：读取定时任务写入数据库的余额快照，可用于绘制权益曲线。

`data` 结构：

| 字段         | 类型                    | 说明                       |
|--------------|-------------------------|----------------------------|
| `snapshots`  | `BalanceSnapshot[]`     | 快照数组                   |
| `hasMore`    | bool                    | 是否存在更多记录           |
| `nextCursor` | string?                 | 下一页游标（`recordedAt`） |

`BalanceSnapshot` 字段：

| 字段         | 说明                 |
|--------------|----------------------|
| `asset`      | 资产类型             |
| `available`  | 可用余额（字符串）   |
| `locked`     | 冻结余额（字符串）   |
| `valuation`  | 总权益（字符串）     |
| `source`     | 数据来源（如 `okx`） |
| `recordedAt` | 记录时间（ISO 8601） |

### 2.3 最新余额快照
- **Method**: `GET`
- **Path**: `/api/account/balances/latest`
- **Query**:
  - `asset` (string, optional，默认 `USDT`)
- **说明**：返回最近一次 `BalanceSnapshot`，若无记录则 `data = null`。

### 2.4 获取初始资金
- **Method**: `GET`
- **Path**: `/api/account/initial-equity`
- **说明**：从配置或数据库读取初始资金，用于计算收益率。

| 字段         | 说明                           |
|--------------|--------------------------------|
| `amount`     | 字符串金额（6 位小数）        |
| `recordedAt` | 记录时间（RFC3339）            |

### 2.5 设置初始资金
- **Method**: `POST`
- **Path**: `/api/account/initial-equity`
- **Body**:
  ```json
  { "amount": 12345.67 }
  ```
- **说明**：更新数据库中的初始资金并返回与 2.4 相同的结构。若 `amount < 0` 则返回错误。

### 2.6 当前持仓
- **Method**: `GET`
- **Path**: `/api/account/positions`
- **说明**：读取本地 `positions` 表中 `closed_at IS NULL` 的仓位。

`PositionSnapshot` 字段（camelCase）：

| 字段            | 类型    | 说明                               |
|-----------------|---------|------------------------------------|
| `instId`        | string  | 交易对                             |
| `posSide`       | string  | 持仓方向（多/空/净）               |
| `tdMode`        | string? | 逐仓/全仓模式                      |
| `side`          | string  | 开仓方向                           |
| `size`          | number  | 仓位数量                           |
| `avgPrice`      | number? | 开仓均价                           |
| `markPx`        | number? | 标记价格                           |
| `margin`        | number? | 占用保证金                         |
| `unrealizedPnl` | number? | 未实现盈亏                         |
| `lastTradeAt`   | string? | 最近成交时间                       |
| `closedAt`      | string? | 平仓时间（未平仓为 `null`）        |
| `actionKind`    | string? | 操作类型（agent/手动等）           |
| `entryOrdId`    | string? | 开仓订单 ID                        |
| `exitOrdId`     | string? | 平仓订单 ID                        |
| `metadata`      | object  | 附加信息（JSON）                   |
| `updatedAt`     | string  | 后端更新时间                       |

### 2.7 历史持仓
- **Method**: `GET`
- **Path**: `/api/account/positions/history`
- **Query**:
  - `symbol` (string, optional)：按交易对过滤
  - `limit` (integer, optional)：限制返回条数
- **说明**：返回已平仓记录，字段同 2.6。

---

## 3. 策略 / Agent 接口

### 3.1 最近策略对话
- **Method**: `GET`
- **Path**: `/api/model/strategy-chat`
- **说明**：读取数据库中最新 15 条策略消息，供前端展示 Agent 摘要。

| 字段                 | 说明                                      |
|----------------------|-------------------------------------------|
| `allowManualTrigger` | bool，是否允许手动触发策略运行            |
| `messages`           | `StrategyMessage[]`                        |

`StrategyMessage`：

| 字段        | 说明                       |
|-------------|----------------------------|
| `id`        | 记录 ID（字符串）          |
| `summary`   | 策略摘要                   |
| `createdAt` | 创建时间（RFC3339）        |

### 3.2 手动触发策略运行
- **Method**: `POST`
- **Path**: `/api/model/strategy-run`
- **Body**: 空
- **说明**：立即异步触发一次策略分析任务。接口返回后任务仍在后台执行，`data` 为 `null`。

```json
{
  "success": true,
  "data": null,
  "error": null
}
```

---

## 4. WebSocket 与轮询

- 行情、账户数据目前均通过 HTTP 轮询获取。
- 策略运行内部通过 WebSocket 与 Agent 通信，对前端透明。若后续需要实时推送，可参照 `market/ticker:{symbol}`、`account/orders` 等频道命名。

---

## 5. 错误码（可选）

默认只返回 `error` 文本；如需精确控制可在响应中附带 `code`，推荐值：

| Code                | 描述             | 建议处理方式              |
|---------------------|------------------|---------------------------|
| `AUTH_REQUIRED`     | 未授权/凭证缺失  | 重定向登录或弹窗提示      |
| `BALANCE_NOT_ENOUGH`| 余额不足         | 提示补充保证金或减仓      |
| `ORDER_REJECTED`    | 交易所拒单       | 展示具体原因并允许调整    |
| `INVALID_PARAMETER` | 参数不合法       | 高亮输入并提示修正        |

---

## 6. 认证与安全

- 推荐为账户、策略类接口启用 Bearer Token（`Authorization: Bearer <token>`）。
- Nginx 层负责 TLS/HTTPS 与限流，后端只暴露 `/api`。
- 前端需要统一处理 401/403，提示重新登录或联系管理员。

---

## 7. 版本管理

- 每次新增或变更接口时同步更新本文档与前端字段映射。
- 建议在 Git 提交说明引用对应章节，便于 Review。
