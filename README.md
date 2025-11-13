
# AiTrader 概览

AiTrader 是一个围绕 OKX 交易所构建的量化交易系统，前端使用 **React + TypeScript**，后端以 **Rust** 提供账户、行情与路由服务，同时通过独立的 **Python Agent** 承载大模型对话与策略分析能力。

系统保持“业务逻辑（Rust）”与“AI 能力（Python）”的明确分层：Rust 负责管理交易所访问与业务 API，Python Agent 直接调用 DeepSeek 模型并通过自身定义的工具访问 OKX。

## 架构一览

```
┌──────────────────────────────┐
│   前端 (React + TypeScript)   │
│ - 行情与账户展示              │
│ - 策略/AI 对话界面            │
└───────────────▲──────────────┘
                │ HTTP
┌───────────────┴──────────────┐
│     Rust API Server (Axum)    │
│ - OKX REST 代理               │
│ - 账户/持仓/成交查询          │
│ - 策略消息记录                │
│ - AI 请求转发                 │
└───────────────▲──────────────┘
                │ HTTP (8001)
┌───────────────┴──────────────┐
│     Python Agent (FastAPI)    │
│ - DeepSeek Chat 接入          │
│ - FastMCP 工具集 (OKX 操作)   │
│ - 策略分析与对话管理          │
└──────────────────────────────┘
                │
         ┌──────┴──────┐
         │   DeepSeek   │
         └──────────────┘
```

## 仓库结构

- `frontend/`：前端单页应用，展示行情、账户以及与 Agent 的对话窗口。
- `backend/`：单一 Rust crate，`src/` 下包含 Axum 入口、配置加载、PostgreSQL 初始化与 OKX 客户端实现。
- `agent/`：Python 端 Agent，包含 FastAPI 服务、DeepSeek 接入、FastMCP 工具与测试脚本。

## 关键能力

### Rust API Server
- 暴露行情、订单簿、成交、账户余额、持仓等 REST 接口（主要来自 OKX 模拟账户）。
- 维护策略运行记录，并提供 `/model/strategy-run` 入口将请求转发给 Python Agent。
- 统一加载 `.env` 中的 OKX 凭证、Agent 地址等配置。

### Python Agent
- 使用 FastAPI 暴露 `/analysis` 端点，面向策略分析和下单决策。
- 通过 FastMCP 定义 OKX 相关函数（订单、行情、账户），由大模型在分析过程中自动调用。
- 提供会话记忆、SSE 扩展点及后续多模型扩展计划。
- 搭配 `tests/` 目录覆盖配置、会话管理、API 路由等关键单元。

## 快速上手

1. **准备环境变量**
   - Rust 服务需要 `OKX_API_KEY`、`OKX_API_SECRET`、`OKX_PASSPHRASE`，是否走模拟盘由 `OKX_USE_SIMULATED` 控制。
   - 通过 `OKX_INST_IDS` 指定需要同步的合约列表（例如 `OKX_INST_IDS=BTC-USDT-SWAP,ETH-USDT-SWAP`），系统会按顺序依次同步；默认仅跟踪 `BTC-USDT-SWAP`。
   - Python Agent 需要 `DEEPSEEK_API_KEY`、`OKX_*`、`AGENT_PORT` 等配置，可将 `agent/.env.example` 复制为仓库根目录下的 `.env` 并填写；同样通过 `OKX_USE_SIMULATED=false` 可切换到实盘（默认开启模拟）。
   - 若希望自动触发策略分析，可设置 `STRATEGY_SCHEDULE_ENABLED=true` 并通过 `STRATEGY_SCHEDULE_INTERVAL_SECS` 指定轮询秒数；该定时器运行在 Rust 服务侧，若检测到正在执行的任务会跳过本次。

2. **启动 Python Agent**
   ```bash
   cd agent
   uv pip install -r requirements.txt -r requirements-dev.txt
   uvicorn llm.main:app --host 0.0.0.0 --port 8001
   ```

3. **启动 Rust API Server**
   ```bash
   cd backend
   cargo run
   ```

4. **前端开发**
   ```bash
   cd frontend
   pnpm install
   pnpm dev
   ```

5. **部署构建（可选）**
   ```bash
   bash nginx/build.sh
   ```
   该脚本会构建后端/前端并在 `agent/.venv` 安装依赖，配合后续的 systemd/nginx setup 直接部署产物；数据库配置由 `.env` 提供，不再由脚本注入。

## 后续工作

- 为 API Server 增加更多 OKX 账户与交易端点，并补齐测试。
- 在 Python Agent 中拓展工具集（如资金费率、交易执行）并提供端到端回归。
- 逐步将 `new.md` 中的设计内容合并进正式文档与 README，删除临时文件。
