# AiTrader Agent 服务

Python 版 **AiTrader Agent** 负责承接 Rust 后端转发的 AI 对话请求、调用 DeepSeek 大模型，并通过 FastMCP 工具直接操作 OKX API。Agent 与 Rust 服务解耦，自行管理配置、日志以及对外 HTTP 能力，便于独立部署和扩缩容。

- FastAPI 提供健康检查、对话与分析 API。
- DeepSeek 客户端集成 FastMCP，实现自动工具编排。
- OKX REST 封装暴露为工具，支持行情、账户与交易操作。
- `core/` 目录集中存放配置、日志、HTTP 适配器等基础能力。

## 架构概览

```
┌──────────────────────────────────────────────────────────────────┐
│                    前端 (React + TypeScript)                      │
│              - 实时数据展示、交易界面、策略对话                     │
└──────────────────────────────────────────────────────────────────┘
                              ↑ HTTP
                              │
┌──────────────────────────────────────────────────────────────────┐
│                   Rust Backend (api-server)                       │
│  - 数据存储 (PostgreSQL/Redis)                                    │
│  - OKX API 交互                                                   │
│  - 账户管理、订单管理、市场数据查询                                │
│  - 直接响应前端查询请求                                            │
│  - AI 请求代理 (转发到 Agent)                                     │
└──────────────────────────────────────────────────────────────────┘
                              ↓ HTTP (AI对话)
                              │
                 ┌────────────────────────────────┐
                 │   Python Agent 服务 (8001)     │
                 │  ┌──────────────────────────┐  │
                 │  │  LLM Module              │  │
                 │  │ - DeepSeek API           │  │
                 │  │ - 对话管理               │  │
                 │  │ - FastMCP Tools          │  │
                 │  │ - Prompt 管理            │  │
                 │  └──────────────────────────┘  │
                 │       ↓              ↓          │
                 │  DeepSeek API    OKX API       │
                 │  (LLM调用)      (直接调用)      │
                 └────────────────────────────────┘
```

**职责拆分**
- 前端只与 Rust 交互，AI 请求由 Rust 转发到 Agent。
- Agent 独立管理 OKX Key，通过 FastMCP 定义并调用工具。
- 与 Rust 解耦，可单独扩缩容或灰度升级。

## 目录结构

```
agent/
├── README.md                          # 本文档
├── requirements.txt                   # 运行+开发依赖
├── core/                              # 公共基础组件
├── llm/                               # FastAPI 与 LLM 业务
├── mcp/                               # FastMCP Server 与工具
├── scripts/                           # 运维脚本
└── tests/                             # 单元测试
```

- `core/`: 配置、日志、HTTP 客户端、OKX 封装与通用类型。  
- `llm/`: FastAPI 入口、路由、业务服务、Prompt 管理。  
- `mcp/`: FastMCP Server 与工具注册，复用 `core.okx_client`。  
- `scripts/`: 启动与健康检查辅助脚本。  
- `tests/`: 核心功能回归测试，覆盖策略分析流程与工具调用。

## MCP 工具列表

| 名称 | 说明 |
| --- | --- |
| `get_ticker` | 获取指定交易对实时行情 |
| `get_account_balance` | 获取账户余额 |
| `get_positions` | 获取持仓信息 |
| `place_order` | 提交交易订单 |
| `cancel_order` | 撤销交易订单 |
| `get_order_history` | 查询历史订单记录 |

### `place_order` 工具详细说明

`place_order` 对应 OKX `/api/v5/trade/order`，参数完全可选地对齐 OKX REST 规范，FastMCP 通过 Pydantic `PlaceOrderInput` 提供字段说明与 schema：

| 字段 | 描述 | 示例 |
| --- | --- | --- |
| `instId` | 交易产品 ID | `BTC-USDT-SWAP` |
| `tdMode` | 交易模式，`cross`/`isolated`/`cash` | `cross` |
| `side` | 买卖方向，`buy` 表示做多、`sell` 表示做空 | `buy` |
| `posSide` | 持仓方向（`long` 或 `short`），`SWAP` 合约必填 | `long` |
| `ordType` | 订单类型，如 `market`、`limit` | `market` |
| `sz` | 下单张数/数量（字符串） | `0.1` |
| `px` | 限价单价格，仅限价单需填写 | `106000` |
| `attachAlgoOrds` | 可选一组算法单（如止盈/止损），每个元素需包含 `algoSide`/`algoOrdType`/`triggerPx`/`px`/`ordType` 等字段 | `[{"algoSide":"tp","algoOrdType":"conditional","triggerPx":"108000","px":"108000","ordType":"limit"}]` |
| `reduceOnly` | 是否纯减仓（`true`/`false`） | `false` |

工具 schema 会自动将这些字段暴露给 DeepSeek，LLM 在请求 `place_order` 前应：
1. 先用 `get_positions` 确认当前持仓方向与数量。
2. 决定方向后填写 `side` 与对应 `posSide` (`buy`→`long`、`sell`→`short`)。
3. 附上 `sz`、`tdMode`，若为限价单再提供 `px`。
4. 若需要止盈止损，可手动指定 `attachAlgoOrds`；否则将只是单纯的市价/限价主单。
5. 添加 `reduceOnly: true` 表示只减仓，默认可省略。

示例：

```json
{
  "order": {
    "instId": "BTC-USDT-SWAP",
    "tdMode": "cross",
    "side": "buy",
    "posSide": "long",
    "ordType": "market",
    "sz": "0.1"
  }
}
```

工具通过 FastMCP 自动生成 Schema，函数签名与 Pydantic 模型同时负责参数校验与文档描述。

## HTTP 交互协议

### 前端 ↔ Rust Backend（常规查询）
```
GET /api/account/balance
GET /api/account/positions
GET /api/market/ticker
GET /api/account/orders/open
...
```

### 前端 ↔ Rust Backend ↔ Agent（策略分析）
1. 前端调用 `POST /api/ai/analysis`
2. Rust 转发到 `POST http://agent:8001/analysis/`
3. Agent 完成 DeepSeek 推理与工具调用后返回分析结果

### Agent ↔ OKX API
FastMCP 工具内部直接调用 OKX REST：
- 公共行情接口无需签名
- 账户、交易接口自动添加签名与 `x-simulated-trading: 1`
- 响应统一通过 `wrap_response` 规整为字典

## 快速开始

1. 复制环境变量示例并按需修改（在仓库根目录）：
   ```bash
   cp .env.example .env
   # 填入 DEEPSEEK、OKX 等密钥，日志路径可通过 LOG_FILE 指定
   ```

2. 准备虚拟环境并安装依赖：
   ```bash
   cd agent
   python -m venv .venv
   source .venv/bin/activate  # Windows 使用 .venv\Scripts\activate
   pip install -r requirements.txt
   ```
   若已执行 `nginx/build.sh`，上述步骤已自动完成，可跳过安装环节。

3. 启动 Agent：
   ```bash
   python -m agent.scripts.run_agent
   # 或在已激活虚拟环境下
   uvicorn agent.llm.main:app --reload --host 0.0.0.0 --port 8001
   ```
   默认日志写入仓库根目录 `log/agent.log`，可在 `.env` 中调整。

## 核心模块说明

- `core.config`：基于 `pydantic-settings` 读取配置，支持 `.env` 链式查找与类型校验。  
- `core.logging_config`：structlog 配置，输出到 stdout 与 `log/agent.log`。  
- `core.okx_client`：OKX REST 封装，负责签名、重试与错误处理。  
- `llm.services.deepseek_client`：对 DeepSeek 的封装，内置 `httpx` + `tenacity` 重试。  
- `llm.services.conversation_manager`：内存态会话存储，支持历史截断。  
- `llm.api.analysis`：策略分析入口，协调 LLM 推理与工具调用。  
- `mcp.server`：FastMCP Server 单例，负责工具注册、Schema 缓存与工具执行。  
- `scripts.run_agent`：本地启动脚本，自动复用 `agent/.venv` 并启用热重载。

## 配置说明

常用环境变量（详见 `.env.example`）：

- `DEEPSEEK_API_KEY` / `DEEPSEEK_API_BASE`：大模型密钥与地址  
- `OKX_API_KEY` / `OKX_SECRET_KEY` / `OKX_PASSPHRASE`：OKX 凭证（支持模拟盘）  
- `OKX_BASE_URL`：OKX API 基地址  
- `OKX_USE_SIMULATED`：是否对请求附带 `X-SIMULATED-TRADING: 1`（默认开启，设为 `false` 走实盘）  
- `AGENT_HOST` / `AGENT_PORT`：FastAPI 监听地址  
- `AGENT_BASE_URL`：提供给 Rust 的 Agent 地址（后端读取）  
- `RUST_WS_URL`：Rust API 的 WebSocket 事件通道，agent 会在 `/analysis/` 业务完成后推送 `task_result`（示例 `ws://rust:3000/agent/ws`）  
- `RUST_WS_TIMEOUT_SECONDS`：等待 Rust WebSocket ACK 的超时时间（默认 5 秒）  
- `LOG_FILE`：日志文件路径，默认 `log/agent.log`

## 测试

```bash
cd agent
python -m venv .venv && source .venv/bin/activate  # 若尚未创建
pip install -r requirements.txt
pytest
```

## 技术栈

- FastAPI / Uvicorn：异步 Web 服务与 OpenAPI 文档  
- FastMCP 2.x：工具注册、Schema 管理与调用链编排  
- httpx：异步 HTTP 客户端  
- structlog：结构化日志  
- Pydantic v2 + pydantic-settings：配置与数据模型  
- tenacity、pytest、ruff、mypy：可靠性与质量保障工具链

## 开发计划

- 丰富工具请求/响应模型，增强错误分类与提示。  
- 引入 Redis 等持久化层，支持对话历史跨实例共享。  
- 建立 CI：`ruff`、`mypy`、异步接口集成测试。  
- 拓展市场分析能力（资金费率、希腊值等）。  
- 评估会话历史持久化方案，完善水平扩展策略。
