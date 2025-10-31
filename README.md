
# AiTrader 设计文档

## 1. **项目概述**

**AiTrader** 是一个基于大模型和市场数据分析的自动化加密货币交易系统，使用 **Rust** 进行后端开发，并结合 **React + TypeScript** 作为前端展示。系统的主要功能包括获取市场数据、账户状态、执行交易、展示交易结果和收益、以及风险管理等。

## 2. **架构设计**

```
┌──────────────────────────────────────┐
│           前端（React + TS）           │
│  - 实时数据展示                        │
│  - 当前持仓、盈亏展示                  │
│  - 交易历史及决策日志                  │
└──────────────────────────────────────┘
                 ↑
                 │
┌──────────────────────────────────────┐
│            后端（Rust）               │
│  - 数据存储（PostgreSQL / Redis）     │
│  - MCP 服务接口                        │
│  - 大模型调用接口（DeepSeek）         │
│  - OKX API 接入                         │
└──────────────────────────────────────┘
```

### 2.1 后端技术栈
- **Rust**：后端开发，处理业务逻辑和API请求。
- **PostgreSQL / Redis**：数据存储。PostgreSQL 用于存储历史数据和账户信息，Redis 用于缓存实时市场数据（如果需要）。
- **DeepSeek API**：大模型的接入，用于决策分析和交易策略生成。
- **OKX API**：交易所接口，获取市场数据和执行交易。

---

## 3. **主要功能**

### 3.1 **市场数据获取**
- **功能**：实时获取市场数据，包括币种价格、技术指标（如EMA、MACD、RSI等）、资金费率、开仓量等。
- **API 调用**：`get_market_data`
- **参数**：
  ```json
  {
    "coins": ["BTC", "ETH", "SOL", "BNB", "DOGE", "XRP"],
    "timeframe": "3m",
    "indicators": ["price", "ema20", "ema50", "macd", "rsi7", "rsi14"],
    "include_orderbook": false,
    "include_funding": true,
    "include_open_interest": true
  }
  ```
- **返回数据结构**：
  ```json
  {
    "timestamp": "2025-10-23T01:35:25.719433",
    "coins": {
      "BTC": {
        "current_price": 108284.5,
        "current_ema20": 108046.502,
        "current_macd": 157.88,
        "current_rsi": 66.683,
        "open_interest": {
          "latest": 24099.11,
          "average": 23509.7
        },
        "funding_rate": 0.0000125,
        "price_series": [107853.5, 107957.5, 108238.5]
      }
    }
  }
  ```

### 3.2 **账户状态获取**
- **功能**：获取账户信息，包括账户总值、可用现金、盈亏情况和持仓信息。
- **API 调用**：`get_account_state`
- **参数**：
  ```json
  {
    "include_positions": true,
    "include_history": true,
    "include_performance": true
  }
  ```
- **返回数据结构**：
  ```json
  {
    "account_value": 10779.03,
    "available_cash": 4434.73,
    "total_pnl": 779.03,
    "total_fees": 136.60,
    "sharpe_ratio": 1.097,
    "win_rate": 0.111,
    "active_positions": [
      {
        "coin": "XRP",
        "side": "long",
        "entry_price": 2.34,
        "entry_time": "2025-10-22T06:24:35",
        "quantity": 6837,
        "leverage": 20,
        "liquidation_price": 2.28,
        "unrealized_pnl": 223.91,
        "current_price": 2.37,
        "exit_plan": {
          "profit_target": 2.45,
          "stop_loss": 2.28
        }
      }
    ]
  }
  ```

### 3.3 **交易执行**
- **功能**：执行开仓、平仓交易操作。
- **API 调用**：`execute_trade`
- **参数**：
  ```json
  {
    "action": "open_long",  // open_long, open_short, close_position
    "coin": "BTC",
    "leverage": 10,
    "margin_amount": 1000,
    "exit_plan": {
      "profit_target": 112253.96,
      "stop_loss": 105877.7,
      "invalidation_condition": "4-hour close below 105000"
    },
    "confidence": 75
  }
  ```
- **返回数据**：
  ```json
  {
    "success": true,
    "position_id": "pos_abc123",
    "entry_price": 108284.5,
    "quantity": 0.092,
    "notional_value": 10000,
    "liquidation_price": 97941.0,
    "message": "Position opened successfully"
  }
  ```

### 3.4 **退出计划更新**
- **功能**：更新现有持仓的退出计划（止盈、止损、失效条件等）。
- **API 调用**：`update_exit_plan`
- **参数**：
  ```json
  {
    "position_id": "pos_abc123",
    "new_profit_target": 115000,
    "new_stop_loss": 106000,
    "new_invalidation": "4-hour close below 105500"
  }
  ```

### 3.5 **交易表现指标**
- **功能**：获取交易的表现指标，如Sharpe Ratio、胜率、最大盈利和最大亏损等。
- **API 调用**：`get_performance_metrics`
- **返回数据**：
  ```json
  {
    "sharpe_ratio": 1.097,
    "win_rate": 0.111,
    "average_leverage": 12.7,
    "average_confidence": 69.8,
    "biggest_win": 1490,
    "biggest_loss": -455.66,
    "hold_times": {
      "long": 0.936,
      "short": 0.05,
      "flat": 0.013
    }
  }
  ```

---

## 4. **大模型接入（DeepSeek）**

### 4.1 **DeepSeek 模型集成**
- **功能**：AI 决策模块，使用 DeepSeek 模型生成交易决策。
- **决策流程**：
  1. 收集市场数据、账户状态、历史决策数据。
  2. 构建用户提示词（`user_prompt`）并发送给 DeepSeek 模型。
  3. 获取模型输出的交易决策（开仓、平仓、止盈、止损等）。
  4. 执行相应的交易操作。
- **调用 DeepSeek 模型**：通过 `openai.ChatCompletion.create` 或其他接口调用 DeepSeek 模型获取交易决策。

### 4.2 **交易决策输出格式**
- **简短总结**（Public Summary）
- **决策行动**（通过工具调用执行）
- **信心度**（Confidence）
- **内部推理**（Internal Reasoning）

---

## 5. **数据存储与管理**

### 5.1 **存储设计**
- **数据库**：使用 PostgreSQL 存储市场数据、账户信息、交易记录和历史决策。
- **缓存**：使用 Redis 缓存实时市场数据（可选），以减少高频数据存取对数据库的负担。
- **数据表设计**：
  - **账户信息表**：存储账户总值、可用资金、总盈亏等。
  - **交易记录表**：存储每笔交易的细节（开盘时间、平仓时间、盈亏等）。
  - **市场数据表**：存储实时的市场数据（币种、价格、技术指标等）。

### 5.2 **数据更新与同步**
- 定期更新市场数据和账户信息。
- 在每次交易决策后，更新账户的持仓状态和盈亏情况。

---

## 6. **前端设计**

### 6.1 **前端展示功能**
- **功能**：使用 React + TypeScript 展示当前账户状态、持仓情况、未实现盈亏、交易历史等。
- **实时更新**：前端定期从后端获取最新的市场数据、账户状态，并展示在用户界面中。
- **用户交互**：用户可以查看每个合约的详细信息、盈亏、止盈止损等，也可以通过前端与系统进行交互（如调整止盈止损、查看交易历史等）。

---

## 7. **风控设计**

### 7.1 **风控规则**
- **最大杠杆限制**：不超过 25X。
- **单笔交易最大规模**：每笔交易不超过账户总值的 50%。
- **总风险敞口**：所有持仓的风险敞口不超过账户总值的 90%。
- **现金储备**：始终保持至少 5% 的现金储备。

### 7.2 **风险评估与决策**
- **AI 决策**：基于市场数据和账户状态，AI 将判断是否开仓、平仓、调整止盈止损。
- **退出计划**：每个持仓必须有明确的止盈、止损和失效条件。

---

## 8. **总结**

1. **系统设计**：使用 Rust 编写后端，集成 DeepSeek 模型进行交易决策，结合 OKX API 执行交易。
2. **市场数据与账户状态**：通过 API 获取实时市场数据和账户信息，使用 PostgreSQL 存储数据，Redis 缓存实时数据（可选）。
3. **前端展示**：使用 React + TypeScript 实现实时账户状态展示。
4. **风控设计**：设定杠杆、风险敞口和现金储备的限制，确保系统的稳健性和风险控制。

---

## 9. **MCP 本地调试流程**

1. **准备环境**
   - 确保已安装 Rust toolchain（`rustup` / `cargo`）。
   - 确保已安装 Node.js（用于运行 `npx` 命令）。
2. **启动演示服务器**
   - 在项目根目录进入 `backend/`，执行：
     ```bash
     cd backend
     npx @modelcontextprotocol/inspector cargo run -p mcp_adapter --bin mcp-demo-server
     ```
   - MCP Inspector 会编译并运行 demo server，同时在终端输出一个本地调试页面地址（如 `http://localhost:3000`）。
3. **通过 Inspector 验证**
   - 打开终端给出的 URL；
   - 在页面中选择 Transport = `STDIO`，点击 “Connect”；
   - 连接成功后切换到 “Tools” 标签，选择 `one_plus_one` 并点击 “Call Tool”，预期返回值为 `2`，即表明 `rmcp` 集成正常工作。
