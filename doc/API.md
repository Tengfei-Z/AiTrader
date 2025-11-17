# AiTrader 前后端接口说明

本文档约定后端 Rust/Axum API 与前端交互的 HTTP 契约。所有接口默认返回 `application/json`，并以标准化响应结构：

```json
{
  "success": true,
  "data": { ... },
  "error": null
}
```

当 `success` 为 `false` 时，`error` 字段包含错误描述，`data` 置为 `null`。

> **说明**：以下路径均以 `/api` 作为前缀（由 Nginx / Axum 路由统一处理）。

---

## 1. 行情相关

### 1.1 获取单个交易对最新行情（基于 K 线窗口）

- **Method**: `GET`
- **Path**: `/api/market/ticker`
- **Query 参数**:
  - `symbol` (string, required): 交易对标识，如 `BTC-USDT`
- **说明**：返回的数据来自最新一根 OKX K 线（周期由 `OKX_TICKER_BAR` 控制，默认为 `3m`，支持 OKX 文档中的 `bar` 取值如 `1m`、`5m`、`1H` 等）。`high24h` / `low24h` / `vol24h` 等字段表示该窗口内的统计值。

**响应示例**

```json
{
  "success": true,
  "data": {
    "symbol": "BTC-USDT",
    "bar": "3m",
    "last": "112391.1",
    "open24h": "112300.5",
    "bidPx": "112391.1",
    "askPx": "112391.2",
    "high24h": "115590",
    "low24h": "112084.7",
    "vol24h": "8637.6433954",
    "volCcy24h": "969220342.1",
    "timestamp": "1761750515009"
  },
  "error": null
}
```

### 1.2 获取盘口深度

- **Method**: `GET`
- **Path**: `/api/market/orderbook`
- **Query 参数**:
  - `symbol` (string, required)
  - `depth` (integer, optional, default `50`): 返回档位数量

**响应示例**

```json
{
  "success": true,
  "data": {
    "bids": [
      ["112391.1", "0.35"],
      ["112391.0", "1.02"]
    ],
    "asks": [
      ["112391.2", "0.48"],
      ["112391.5", "0.27"]
    ],
    "timestamp": "1761750515009"
  },
  "error": null
}
```

### 1.3 获取近期成交

- **Method**: `GET`
- **Path**: `/api/market/trades`
- **Query 参数**:
  - `symbol` (string, required)
  - `limit` (integer, optional, default `50`, max `200`)

**响应示例**

```json
{
  "success": true,
  "data": [
    {
      "tradeId": "123456789",
      "price": "112391.1",
      "size": "0.01",
      "side": "buy",
      "timestamp": "1761750514000"
    }
  ],
  "error": null
}
```

---

## 2. 账户与订单

### 2.1 查询账户余额

- **Method**: `GET`
- **Path**: `/api/account/balances`

**响应示例**

```json
{
  "success": true,
  "data": [
    {
      "asset": "BTC",
      "available": "0.523",
      "locked": "0.05",
      "valuationUSDT": "58768.23"
    },
    {
      "asset": "USDT",
      "available": "15432.5",
      "locked": "1500",
      "valuationUSDT": "16932.5"
    }
  ],
  "error": null
}
```

### 2.2 查询未完成订单

- **Method**: `GET`
- **Path**: `/api/account/orders/open`
- **Query 参数**:
  - `symbol` (string, optional)

**响应示例**

```json
{
  "success": true,
  "data": [
    {
      "orderId": "123456",
      "symbol": "BTC-USDT",
      "side": "buy",
      "type": "limit",
      "price": "110000",
      "size": "0.05",
      "filledSize": "0.02",
      "status": "partially_filled",
      "createdAt": "1761750300000"
    }
  ],
  "error": null
}
```

### 2.3 查询历史订单

- **Method**: `GET`
- **Path**: `/api/account/orders/history`
- **Query 参数**:
  - `symbol` (string, optional)
  - `limit` (integer, optional, default `50`)
  - `state` (string, optional): `filled`, `canceled`, `all`
  - `from`, `to` (timestamp, optional): 时间范围

### 2.4 查询成交记录

- **Method**: `GET`
- **Path**: `/api/account/fills`
- **Query 参数**:
  - `symbol` (string, optional)
  - `limit` (integer, optional, default `50`)

**响应示例**

```json
{
  "success": true,
  "data": [
    {
      "fillId": "987654",
      "orderId": "123456",
      "symbol": "BTC-USDT",
      "side": "buy",
      "price": "109500",
      "size": "0.03",
      "fee": "0.000015",
      "timestamp": "1761750400000"
    }
  ],
  "error": null
}
```

---

## 3. 统一错误码（可选）

可在响应中附加 `code` 字段表明错误类型，例如：

| Code | 描述                 | 建议处理方式                |
|------|----------------------|-----------------------------|
| `AUTH_REQUIRED` | 未授权或凭证缺失       | 前端重定向到登录或提示       |
| `BALANCE_NOT_ENOUGH` | 余额不足             | 弹窗提醒用户补充余额          |
| `ORDER_REJECTED` | 交易所拒单             | 提示原因并允许用户调整参数    |
| `INVALID_PARAMETER` | 参数不合法             | 高亮表单字段、提示修正        |

---

## 5. WebSocket（预留）

若后续支持实时订阅，可约定以下频道：

- `market/ticker:{symbol}`
- `market/orderbook:{symbol}`
- `account/orders`

目前前端以轮询方式实现，待后端支持后再补充协议说明。

---

## 6. 认证与安全

- 推荐通过 API Key / Token 机制保护账户类接口（前端需在请求头携带 `Authorization: Bearer <token>`）。
- Nginx 层应处理 HTTPS/TLS。
- 前端需要统一处理 401/403 响应，提示重新登录或联系管理员。

---

## 7. 版本管理

建议在后端新增变更时更新此文档，并在前端使用前确认字段是否同步。
