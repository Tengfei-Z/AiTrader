
# AiTrader

AiTrader 是一个围绕 OKX 交易所构建的量化交易系统：前端基于 **React + TypeScript** 提供交易与 AI 对话界面，后端使用 **Rust (Axum)** 统一接入 OKX 与自研业务 API，独立的 **Python Agent** 则承载 DeepSeek 模型对话、策略分析与工具调用。整体强调“交易业务（Rust）”与“AI 能力（Python）”的清晰分层。

## 技术栈速览

| 模块 | 技术 | 角色 |
| --- | --- | --- |
| 前端 | React, TypeScript, Vite, pnpm | 行情与账户展示、策略与 AI 对话 |
| API 服务 | Rust, Axum, sqlx, PostgreSQL | OKX REST 代理、账户/行情/策略 API、任务调度 |
| Agent | Python, FastAPI, FastMCP, DeepSeek | 大模型推理、OKX 工具调用、策略分析 |
| 部署 | systemd, nginx, bash scripts | 一键构建与服务编排 |

## 架构

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

- `frontend/`：React 单页应用，覆盖行情看板、账户/持仓视图与 Agent 对话窗口。
- `backend/`：Rust crate，内含 Axum 入口、配置加载、PostgreSQL 初始化、OKX 客户端与任务调度。
- `agent/`：Python Agent，包含 FastAPI 服务、DeepSeek 接入、FastMCP 工具、策略脚本与测试。
- `config/`、`nginx/`、`doc/`：配置模板、部署脚本与设计文档。

## 核心服务

### Rust API Server
- 提供行情、订单簿、成交、账户余额、持仓等 REST 接口（默认对接 OKX 模拟盘）。
- 维护策略运行/对话记录，并通过 `/model/strategy-run` 等端点转发请求给 Python Agent。
- 统一加载 `.env`，管理 OKX 凭证、Agent 地址、数据库 URL、定时任务等配置。

### Python Agent
- FastAPI 暴露 `/analysis` 等端点，驱动 DeepSeek Chat 进行策略分析与自然语言交互。
- 借助 FastMCP 定义下单、行情、账户等 OKX 工具，让模型可在推理中自动调用。
- 提供会话记忆、SSE 扩展点，`tests/` 中覆盖配置、会话管理与 API 路由单测。

### 前端
- 使用 React + TypeScript + Vite，配合 pnpm 管理依赖。
- 与 Rust API 拉取行情/账户数据，并通过 SSE/HTTP 与 Agent 交互。
- UI 重点在策略监控与人工干预入口。

## 配置与环境变量

- **OKX 凭证**：`OKX_API_KEY`、`OKX_API_SECRET`、`OKX_PASSPHRASE`。`OKX_USE_SIMULATED` 控制是否启用模拟盘（默认 true）。
- **合约列表**：`OKX_INST_IDS=BTC-USDT-SWAP,ETH-USDT-SWAP` 等，用于指定需要同步的合约；默认仅跟踪 `BTC-USDT-SWAP`。
- **Agent**：`DEEPSEEK_API_KEY`、`AGENT_PORT`、`AGENT_HOST` 等；可将 `agent/.env.example` 复制为仓库根目录 `.env` 并补齐。
- **调度**：`STRATEGY_SCHEDULE_ENABLED=true` 与 `STRATEGY_SCHEDULE_INTERVAL_SECS=60` 控制 Rust 端的策略轮询，若检测到已有任务在执行，会自动跳过。
- **数据库**：`DATABASE_URL` 由 `.env` 提供，`backend` 启动时自动迁移/初始化。
- **初始资金与快照压缩**：
  - `INITIAL_EQUITY` 除了决定前端默认基线，也会在数据库缺少记录时自动写入 `initial_equities` 表（任何重复写入会覆盖旧值，表内最多一条）。
  - `BALANCE_SNAPSHOT_MIN_ABS_CHANGE` / `BALANCE_SNAPSHOT_MIN_RELATIVE_CHANGE` 控制账户快照写入的阈值（默认 1 USDT / 0.01%）。只有当“绝对变化 < abs 阈值”且“相对变化 < rel 阈值”同时成立时才会跳过写入，否则即使满足其中一个条件也会记录，以避免遗漏较大波动。

## 快速上手

1. **安装依赖**
   - Rust stable toolchain、cargo、PostgreSQL。
   - Python 3.11+，推荐使用 `uv` 或 `pip` 创建虚拟环境。
   - Node.js 18+ 与 pnpm。
2. **准备 `.env`**
   - 复制 `agent/.env.example` 到仓库根目录 `.env`，补齐 OKX、DeepSeek、数据库、Agent 端口等变量。
3. **启动 Python Agent**
   ```bash
   cd agent
   uv pip install -r requirements.txt -r requirements-dev.txt
   uvicorn llm.main:app --host 0.0.0.0 --port 8001
   ```
4. **启动 Rust API Server**
   ```bash
   cd backend
   cargo run
   ```
5. **启动前端**
   ```bash
   cd frontend
   pnpm install
   pnpm dev
   ```

## 构建与部署

```bash
bash nginx/build.sh
```

脚本会构建前端与后端、在 `agent/.venv` 安装依赖，并输出可直接用于 systemd + nginx 的产物；数据库连接信息完全来自 `.env`。
