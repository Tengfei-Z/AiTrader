# AiTrader Agent 服务

Python 版 **AiTrader Agent** 负责承接 Rust 后端转发的 AI 对话请求、调用 DeepSeek 大模型，并通过 FastMCP Tools 直接对接 OKX API。模块保持与 Rust 服务解耦，自行管理配置、日志和对外 HTTP 交互能力。

- FastAPI 提供健康检查、对话和分析 API。
- DeepSeek 客户端集成 FastMCP，实现工具调用自动编排。
- OKX REST 封装暴露为工具，支持行情、账户、交易操作。
- `core/` 中集中存放配置、日志、HTTP 适配器等基础能力。

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
                 │  │ - Prompt管理             │  │
                 │  └──────────────────────────┘  │
                 │       ↓              ↓          │
                 │  DeepSeek API    OKX API       │
                 │  (LLM调用)      (直接调用)      │
                 └────────────────────────────────┘
```

**职责拆分**  
- 前端只与 Rust 交互，AI 请求由 Rust 转发到 Agent。  
- Agent 独立管理 OKX Key，通过 FastMCP 定义并调用工具。  
- 与 Rust 解耦，可单独扩缩容。

## 目录结构

```
agent/
├── README.md                          # 本文档
├── requirements*.txt                  # 依赖清单
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
- `tests/`: 核心功能回归测试，覆盖对话流程与工具调用。

## 快速开始

1. 复制环境变量示例（在仓库根目录执行）：

   ```bash
   cp agent/.env.example .env
   # 按需填入 DEEPSEEK、OKX 等密钥
    # 如需自定义日志文件，设置 LOG_FILE=/path/to/agent.log
    # 只使用 OKX 模拟盘时，可只填写 OKX_SIM_*，Agent 会自动使用这些值
   ```

2. 准备虚拟环境并安装依赖（切换到 agent 目录）：

   ```bash
   cd agent
   uv venv
   source .venv/bin/activate
   uv pip install -r requirements.txt
   uv pip install -r requirements-dev.txt
   ```

3. 启动服务：

   ```bash
   uvicorn llm.main:app --reload --host 0.0.0.0 --port 8001
   # 或使用脚本：
   python -m agent.scripts.run_agent
   ```

## 核心模块说明

- `core.config`: 基于 `pydantic-settings` 读取环境变量，统一配置入口。  
- `core.logging_config`: structlog 日志配置，支持同时输出到标准输出与 `logs/agent.log`。  
- `core.okx_client`: HTTP 请求签名、重试与错误封装（默认加 `x-simulated-trading: 1`，仅支持 OKX 模拟盘），提供行情/账户/交易接口。  
- `llm.services.deepseek_client`: 使用 `httpx` + `tenacity` 对 DeepSeek 进行多次重试。  
- `llm.services.conversation_manager`: 内存会话存储，支持按 session_id 限制历史长度。  
- `llm.api.chat`: 对话主入口，自动处理工具调用与历史上下文。  
- `mcp.server`: 注册 OKX 工具，缓存 Schema，并提供工具执行入口。

## 当前进度

- FastAPI 应用、DeepSeek 客户端、OKX 工具已实现并可运行。  
- MCP 工具 Schema 缓存、对话上下文管理、基础测试均已就绪。  
- 仍为内存态服务，尚未接入外部存储或分布式部署。

## 后续计划

- 扩展工具模型与错误类型校验，细化 Request/Response 数据结构。  
- 引入 Redis 等持久化对话存储，支持多实例水平扩展。  
- 完成 CI：包含 `ruff`、`mypy`、异步接口集成测试。  
- 丰富分析能力，增加资金费率、希腊值等市场数据。  
- 评估将会话历史迁移至缓存或数据库的同步策略。

## 测试

```bash
cd agent
uv pip install -r requirements.txt
uv pip install -r requirements-dev.txt
pytest
```

## 功能列表

- `GET /health`: 服务健康检查。  
- `POST /chat`: 对话接口，支持自动工具调用与历史追踪。  
- `POST /analysis`: 市场分析接口（复用对话能力）。  
- FastMCP 工具：行情、订单簿、K 线、账户、持仓、委托、下单、撤单（仅向 OKX 模拟盘发起操作）。
