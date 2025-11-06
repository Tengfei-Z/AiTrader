# 设计文档合并说明

完整的架构概述、目录结构、核心模块拆解等内容，已经合并进 `README.md`，请直接参考：

- `README.md` → 项目简介、架构图、目录结构、核心模块说明、测试与后续计划。

若需单独的设计记录，可在此文件中追加章节，但默认情况下以 README 为唯一权威文档，避免信息重复。

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
