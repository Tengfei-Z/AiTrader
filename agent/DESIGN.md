# Agent 模块设计文档

## 1. 架构概述

将原有 Rust 实现的 DeepSeek 大模型调用和 MCP 服务改造为 Python 实现,通过 HTTP API 与 Rust 后端进行交互。

### 1.1 新架构图

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

**说明**:
- **前端 → Rust**: 所有数据查询(账户、市场、订单等)直接请求 Rust Backend
- **前端 → Rust → Agent**: 只有 AI 对话和策略分析请求转发到 Agent
- **Agent → OKX API**: Agent 的 FastMCP Tools 直接调用 OKX API (完全独立)
- **Agent 职责**: 纯粹的 AI 能力，通过 FastMCP 定义工具，直接对接 OKX
- **完全独立**: Agent 不依赖 Rust Backend，各自管理自己的 OKX API Key

## 2. 目录结构设计

```
agent/
├── README.md                          # Agent 模块说明文档
├── DESIGN.md                          # 本设计文档
├── requirements.txt                   # Python 依赖
├── pyproject.toml                     # Python 项目配置
├── .env.example                       # 环境变量示例
│
├── core/                              # 核心公共模块
│   ├── __init__.py
│   ├── config.py                      # 配置管理 (从环境变量加载)
│   ├── logging_config.py              # 日志配置 (structlog)
│   ├── http_client.py                 # HTTP 客户端封装 (用于调用 OKX API)
│   ├── okx_client.py                  # OKX API 客户端 (签名、请求封装)
│   ├── types.py                       # 通用类型定义
│   └── exceptions.py                  # 自定义异常
│
├── llm/                               # 大模型模块 (核心)
│   ├── __init__.py
│   ├── main.py                        # FastAPI 应用入口
│   ├── api/                           # HTTP API 路由
│   │   ├── __init__.py
│   │   ├── chat.py                    # 策略对话接口
│   │   ├── analysis.py                # 市场分析接口
│   │   └── health.py                  # 健康检查
│   ├── services/                      # 业务逻辑层
│   │   ├── __init__.py
│   │   ├── deepseek_client.py         # DeepSeek API 客户端封装
│   │   ├── conversation_manager.py    # 对话上下文管理
│   │   └── strategy_analyzer.py       # 策略分析服务
│   ├── models/                        # 数据模型
│   │   ├── __init__.py
│   │   ├── requests.py                # API 请求模型
│   │   └── responses.py               # API 响应模型
│   ├── prompts/                       # Prompt 模板管理
│   │   ├── __init__.py
│   │   ├── trading_assistant.py       # 交易助手 System Prompt
│   │   └── market_analyst.py          # 市场分析 Prompt
│   └── mcp/                           # FastMCP Tools 定义
│       ├── __init__.py
│       ├── server.py                  # FastMCP Server 实例
│       └── tools/                     # MCP Tools 实现
│           ├── __init__.py
│           ├── market.py              # 市场数据相关 Tools (调用 OKX API)
│           ├── account.py             # 账户相关 Tools (调用 OKX API)
│           └── trade.py               # 交易相关 Tools (调用 OKX API)
│
└── scripts/                           # 脚本工具
    ├── start.sh                       # 启动服务
    └── health_check.py                # 健康检查脚本
```

**架构说明**:
- **使用 FastMCP**: 标准化的 MCP 工具实现，自动生成 Tool Schema
- **核心职责**: 
  - 与 DeepSeek API 交互
  - 管理对话上下文和历史
  - 通过 FastMCP 定义和执行 Tools (直接调用 OKX API)
  - 提供策略分析和建议
- **完全独立**: Agent 不依赖 Rust Backend，直接对接 OKX API

## 3. 模块详细设计

### 3.1 Core 核心模块

**职责**: 提供通用的配置、日志、HTTP客户端等基础功能

**主要组件**:
- `config.py`: 从环境变量加载配置 (使用 pydantic-settings)
- `http_client.py`: 封装 httpx 异步客户端，提供通用的 HTTP 请求能力
- `okx_client.py`: OKX API 客户端封装（签名、认证、错误处理）
- `logging_config.py`: 统一的 structlog 日志配置
- `types.py`: 通用的 Pydantic 模型和枚举
- `exceptions.py`: 自定义异常类 (LLMError, OKXError 等)

### 3.2 LLM 模块 (大模型核心)

**职责**: 
- 使用 **OpenAI Python SDK** 与 DeepSeek API 交互
- 管理对话上下文和历史
- 集成 FastMCP Server，自动处理 Function Call
- 提供策略分析和市场洞察

**DeepSeek Client 实现**:
```python
# agent/llm/services/deepseek_client.py
from openai import AsyncOpenAI

class DeepSeekClient:
    def __init__(self, api_key: str, base_url: str = "https://api.deepseek.com"):
        # 使用 OpenAI SDK，但指向 DeepSeek API
        self.client = AsyncOpenAI(
            api_key=api_key,
            base_url=base_url
        )
    
    async def chat(
        self,
        messages: list,
        tools: list = None,
        model: str = "deepseek-chat",
        temperature: float = 0.7,
        max_tokens: int = 4000
    ):
        """调用 DeepSeek API（兼容 OpenAI 接口）"""
        response = await self.client.chat.completions.create(
            model=model,
            messages=messages,
            tools=tools,  # FastMCP 生成的 Tools Schema
            temperature=temperature,
            max_tokens=max_tokens
        )
        return response
```

**说明**: 
- DeepSeek API 完全兼容 OpenAI 的接口规范
- 只需修改 `base_url` 和 `api_key` 即可
- 支持 Function Calling (Tools)

**核心流程**:
1. 接收 Rust 后端转发的对话请求
2. 从上下文管理器获取对话历史
3. 构建 System Prompt + User Message
4. 从 FastMCP Server 获取 Tools Schema
5. 调用 DeepSeek API (带 Tools)
6. 解析响应:
   - 如果是普通回复 → 直接返回
   - 如果是 Function Call → FastMCP 自动执行 Tool (调用 OKX API)
   - 将 Tool 结果再次发给大模型 → 得到最终回复
7. 保存对话历史，返回结果

**API 端点设计**:
```python
# AI 对话（唯一核心接口）
POST /api/v1/chat
# 请求体为空，不需要任何参数

# 健康检查
GET /api/v1/health
```

**说明**:
- **极简设计**: 只需要一个对话接口，请求体为空
- **对话内容**: 通过其他方式传递（如 WebSocket、SSE 等流式连接）
- **session_id**: 由 Agent 内部自动管理
- **完全无状态**: 不需要任何请求参数

### 3.3 FastMCP Tools 定义

**职责**: 使用 FastMCP 定义大模型可以调用的 Tools，直接调用 OKX API

**Tool 清单**:
1. `get_ticker`: 获取实时行情 (OKX API)
2. `get_account_balance`: 获取账户余额 (OKX API)
3. `get_positions`: 获取持仓信息 (OKX API)
4. `place_order`: 下单 (OKX API)
5. `cancel_order`: 撤单 (OKX API)
6. `get_order_history`: 获取历史订单 (OKX API)

**FastMCP Tool 示例**:
```python
# agent/llm/mcp/tools/market.py
from fastmcp import FastMCP

mcp = FastMCP("trading-assistant")

@mcp.tool()
async def get_ticker(
    inst_id: str,
    description: str = "获取指定合约的实时行情数据"
) -> dict:
    """
    获取实时行情数据
    
    Args:
        inst_id: 合约ID，如 BTC-USDT-SWAP
    
    Returns:
        包含价格、成交量等信息的字典
    """
    from core.okx_client import okx_client
    
    # 直接调用 OKX API
    result = await okx_client.get_ticker(inst_id)
    return result

@mcp.tool()
async def get_market_depth(
    inst_id: str,
    sz: int = 20
) -> dict:
    """
    获取市场深度（订单簿）
    
    Args:
        inst_id: 合约ID
        sz: 深度档位数量，默认20
    
    Returns:
        包含买卖盘数据的字典
    """
    from core.okx_client import okx_client
    
    result = await okx_client.get_order_book(inst_id, sz)
    return result
```

**优势**:
- 🎯 **自动 Schema 生成**: FastMCP 自动从函数签名和文档生成 Tool Schema
- 🔧 **类型安全**: 基于 Python 类型注解，自动验证参数
- 📝 **文档友好**: Docstring 自动转换为工具描述
- 🚀 **简化开发**: 只需定义函数，无需手动编写 JSON Schema

## 4. HTTP 交互协议

### 4.1 前端 → Rust Backend (直接查询，不经过 Agent)

```
前端 → Rust Backend API
  /api/account/balance          # 查询余额
  /api/account/positions        # 查询持仓
  /api/market/ticker            # 查询行情
  /api/orders/list              # 查询订单
  ...
```

### 4.2 前端 → Rust Backend → Agent (AI 对话)

**请求流程**:
```
1. 前端发送对话请求到 Rust
   POST /api/ai/chat

2. Rust 转发到 Agent
   POST http://agent:8001/api/v1/chat

3. Agent 返回响应给 Rust

4. Rust 返回给前端
```

**请求示例** (Rust → Agent):
```http
POST http://agent:8001/api/v1/chat
Content-Type: application/json

{}
```

**响应示例** (Agent → Rust):
```json
{
  "status": "ok",
  "data": {
    "response": "准备就绪，请通过流式连接进行对话",
    "session_id": "auto-generated-session-id"
  }
}
```

### 4.3 Agent → OKX API (直接调用)

**说明**: Agent 的 FastMCP Tools 直接调用 OKX API，完全独立于 Rust Backend。

**示例 1: 获取市场数据**
```python
# FastMCP Tool 内部实现
async def get_ticker(inst_id: str):
    # 直接调用 OKX API
    response = await okx_client.get(
        "/api/v5/market/ticker",
        params={"instId": inst_id}
    )
    return response["data"][0]
```

**OKX API 响应**:
```json
{
  "code": "0",
  "msg": "",
  "data": [{
    "instId": "BTC-USDT-SWAP",
    "last": "108284.5",
    "vol24h": "123456.78",
    "ts": "1699999999999"
  }]
}
```

**示例 2: 获取账户余额**
```python
async def get_account_balance():
    # 需要签名的私有 API
    response = await okx_client.get(
        "/api/v5/account/balance",
        auth=True  # 自动签名
    )
    return response["data"][0]
```

**优势**:
- ✅ **完全独立**: Agent 不依赖 Rust Backend
- ✅ **实时数据**: 直接从 OKX 获取最新数据
- ✅ **简化架构**: 减少服务间调用链路

## 5. 配置管理

**环境变量** (`.env`):
```bash
# 服务配置
HOST=0.0.0.0
PORT=8001

# DeepSeek API
DEEPSEEK_API_KEY=sk-xxx
DEEPSEEK_API_BASE=https://api.deepseek.com
DEEPSEEK_MODEL=deepseek-chat
DEEPSEEK_MAX_TOKENS=4000
DEEPSEEK_TEMPERATURE=0.7

# OKX API (Agent 直接调用)
OKX_API_KEY=your-api-key
OKX_SECRET_KEY=your-secret-key
OKX_PASSPHRASE=your-passphrase
OKX_API_BASE=https://www.okx.com
OKX_SIMULATED=true  # 是否使用模拟盘

# 日志配置
LOG_LEVEL=INFO
ENVIRONMENT=development

# CORS (可选)
ALLOWED_ORIGINS=http://localhost:5173,http://localhost:3000
```

## 6. 部署方案

```bash
# 1. 创建虚拟环境
cd agent
python -m venv .venv

# 2. 激活虚拟环境
# Linux/Mac:
source .venv/bin/activate
# Windows:
.venv\Scripts\activate

# 3. 安装依赖
pip install -r requirements.txt

# 4. 配置环境变量
cp .env.example .env
# 编辑 .env 文件

# 5. 启动服务
python -m llm.main
# 或使用脚本
./scripts/start.sh
```

**注意**: 
- Agent 主体使用 **传统 venv** 管理虚拟环境
- FastMCP 相关的开发建议使用 **uv** (更快的包管理工具)

## 7. 技术栈

- **Web 框架**: FastAPI (异步支持,自动生成 OpenAPI 文档)
- **MCP 框架**: FastMCP (标准化的 MCP Tools 实现)
- **HTTP 客户端**: httpx (异步 HTTP 客户端，调用 OKX API)
- **数据验证**: Pydantic v2
- **AI SDK**: openai (兼容 DeepSeek API)
- **日志**: structlog
- **类型检查**: mypy
- **代码格式化**: black + isort

## 8. 实施计划

### Phase 1: 基础框架搭建
- [ ] 创建 agent 目录结构
- [ ] 配置 Python 开发环境 (requirements.txt, pyproject.toml)
- [ ] 实现 Core 模块 (config, logging, http_client)
- [ ] 搭建 FastAPI 应用骨架

### Phase 2: DeepSeek 集成
- [ ] 实现 DeepSeek API 客户端封装
- [ ] 实现对话上下文管理器
- [ ] 定义 MCP Tools Schema (给大模型)
- [ ] 实现基础的对话 API

### Phase 3: FastMCP Tools 实现
- [ ] 安装和配置 FastMCP
- [ ] 实现 OKX API 客户端封装 (签名、错误处理)
- [ ] 使用 FastMCP 定义市场数据 Tools
- [ ] 使用 FastMCP 定义账户管理 Tools
- [ ] 使用 FastMCP 定义交易执行 Tools
- [ ] 集成 FastMCP 到 DeepSeek Client (自动 Function Call)
- [ ] 错误处理和重试机制

### Phase 4: Prompt 工程
- [ ] 设计交易助手 System Prompt
- [ ] 设计市场分析 Prompt
- [ ] 设计风险管理 Prompt
- [ ] Prompt 模板化和管理

### Phase 5: Rust Backend 适配
- [ ] 添加 AI 代理层 (转发对话请求到 Agent)
  - [ ] /api/ai/chat 端点
  - [ ] 错误处理和超时设置
- [ ] 确保现有 API 可被 Agent 调用
  - [ ] 检查认证机制
  - [ ] 确认数据格式兼容
- [ ] 移除 Rust 的 deepseek crate
- [ ] 更新前端 API 调用（如需要）

### Phase 6: 文档和示例
- [ ] 完善 API 文档
- [ ] 编写使用示例
- [ ] 部署说明文档

## 9. 架构优势

### 9.1 为什么 Agent 使用 Python?

1. **AI 生态成熟**: OpenAI SDK、LangChain 等工具链完善，社区资源丰富
2. **开发效率高**: 快速迭代，适合 Prompt 和策略逻辑频繁调整
3. **Prompt 工程友好**: 字符串处理方便，适合复杂的 Prompt 构建
4. **调试方便**: 动态语言，便于调试复杂的 Function Call 和多轮对话

### 9.2 为什么业务逻辑保留在 Rust?

1. **性能关键**: 交易执行、市场数据处理需要极高性能
2. **类型安全**: 金融系统对数据准确性要求极高，Rust 类型系统提供保障
3. **并发优势**: Tokio 异步运行时适合高并发场景 (WebSocket、数据库)
4. **内存安全**: 避免内存泄漏和竞态条件，保证系统稳定性
5. **已有代码**: OKX API 集成、数据库操作等核心代码已经成熟

### 9.3 职责分离的好处

1. **清晰边界**: 
   - Rust: 业务逻辑、数据查询、交易执行
   - Python: AI 能力、对话管理、策略建议
2. **独立扩展**: 
   - AI 流量大时，独立扩展 Agent 服务
   - 交易流量大时，独立扩展 Rust Backend
3. **技术选型灵活**: 
   - 未来可以更换大模型 (GPT-4, Claude, etc.)
   - 不影响核心业务逻辑
4. **测试简化**: 
   - 可以 Mock Rust Backend 来测试 Agent
   - 可以 Mock Agent 来测试 Rust 业务逻辑

## 10. 关键技术点

### 10.1 对话上下文管理

```python
# 示例：内存中管理对话历史
class ConversationManager:
    def __init__(self, max_history: int = 20):
        self.sessions = {}  # session_id -> messages[]
    
    def add_message(self, session_id: str, role: str, content: str):
        """添加消息到历史"""
        
    def get_history(self, session_id: str, limit: int = 10):
        """获取最近的 N 条消息"""
        
    def clear_session(self, session_id: str):
        """清除会话历史"""
```

### 10.2 FastMCP 集成流程

```python
# 1. 定义 FastMCP Tools
from fastmcp import FastMCP

mcp = FastMCP("trading-assistant")

@mcp.tool()
async def get_ticker(inst_id: str) -> dict:
    """获取实时行情"""
    from core.okx_client import okx_client
    return await okx_client.get_ticker(inst_id)

# 2. 获取 Tools Schema (自动生成)
tools_schema = mcp.get_tools_schema()

# 3. 调用 DeepSeek API
response = await openai_client.chat.completions.create(
    model="deepseek-chat",
    messages=messages,
    tools=tools_schema  # FastMCP 自动生成的 Schema
)

# 4. 如果有 Function Call，FastMCP 自动执行
if response.choices[0].message.tool_calls:
    tool_call = response.choices[0].message.tool_calls[0]
    
    # FastMCP 自动调度执行
    result = await mcp.call_tool(
        tool_call.function.name,
        json.loads(tool_call.function.arguments)
    )
    
    # 5. 再次调用大模型
    messages.append({
        "role": "tool",
        "tool_call_id": tool_call.id,
        "content": json.dumps(result)
    })
    
    final_response = await openai_client.chat.completions.create(
        model="deepseek-chat",
        messages=messages
    )
```

### 10.3 错误处理

1. **大模型 API 错误**: 
   - 限流 (Rate Limit): 指数退避重试
   - 超时: 设置合理的超时时间，提示用户
   - Token 超限: 自动截断历史消息

2. **OKX API 错误**:
   - 连接失败: 重试 3 次，返回友好提示
   - 签名错误: 检查 API Key 配置
   - 业务错误 (余额不足等): 将错误信息传递给大模型，让其生成用户友好的回复
   - 限流: 等待后重试

3. **日志和监控**:
   - 使用 structlog 记录结构化日志
   - 记录每次 LLM 调用的 token 使用量
   - 记录 Function Call 执行时间

### 10.4 安全考虑

1. **API Key 保护**: 环境变量，不写入代码
2. **输入验证**: 使用 Pydantic 验证所有请求参数
3. **限流**: 防止滥用 (可使用 slowapi)
4. **CORS**: 配置允许的前端域名
5. **服务间认证**: 可选，使用 JWT 或 API Key

## 11. 后续优化方向

1. **流式响应**: 支持 Server-Sent Events (SSE)，实时返回大模型生成内容
2. **多模型支持**: 抽象 LLM 客户端，支持切换不同大模型
3. **Prompt 版本管理**: 将 Prompt 存储在数据库，支持 A/B 测试
4. **缓存优化**: 对常见问题使用缓存，减少 API 调用
5. **观测性**: 集成 OpenTelemetry，实现链路追踪
