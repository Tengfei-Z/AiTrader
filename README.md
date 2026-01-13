# AiTrader

AiTrader 是一个围绕 OKX 生态打造的量化交易平台。系统将「交易执行」与「AI 策略」解耦：Rust API 服务负责与 OKX、数据库及前端交互，Python Agent 则承载 DeepSeek 模型推理与策略脚本，React 前端提供可视化与人工干预入口。

## 核心组成

| 模块 | 主要技术 | 职责 |
| --- | --- | --- |
| Frontend | React · TypeScript · Vite | 行情/账户看板、策略对话、人工触发入口 |
| Backend | Rust · Axum · sqlx | OKX REST 代理、账户/持仓/成交 API、策略调度、数据库管理 |
| Agent | Python · FastAPI · DeepSeek | LLM 推理、策略分析、下单/行情工具调用 |

三者通过 HTTP/WebSocket 协同：前端调用 Rust API，API 服务在需要策略分析时通过 WebSocket 通知 Agent，Agent 完成分析后写回数据库与 API。所有模块均由 `.env` 驱动，可在模拟盘或实盘之间快速切换。

```
┌─────────────────────────────┐
│        React Frontend        │
│ - 行情/账户/策略对话         │
│ - 手动触发与观测             │
└──────────────▲──────────────┘
               │ HTTPS/WS
┌──────────────┴──────────────┐
│      Rust API (Axum)         │
│ - OKX REST 代理 + DB          │
│ - 策略调度：手动/定时/波动     │
│ - WebSocket → Python Agent   │
└──────────────▲──────────────┘
               │ WS
┌──────────────┴──────────────┐
│     Python Agent (FastAPI)   │
│ - DeepSeek Chat / MCP 工具   │
│ - 策略分析回写数据库/日志     │
└──────────────┬──────────────┘
               │ HTTP/SDK
        ┌──────┴──────┐
        │    OKX/LLM   │
        └──────────────┘
```

## 策略触发概览

后台支持三种触发方式，它们共享同一个执行许可，保证策略分析串行运行：

- **手动触发**：前端或 CLI 直接调用 `/model/strategy-run`。
- **定时触发**：设置 `STRATEGY_SCHEDULE_ENABLED=true` 与 `STRATEGY_SCHEDULE_INTERVAL_SECS`，按「最晚执行时间」模式兜底巡检。
- **波动触发**：开启 `STRATEGY_VOL_TRIGGER_ENABLED` 后，后台会轮询 OKX 行情（REST），维护每个 instId 的 `last_trigger_price` 与 `last_tick_price`。当 `Δ=|price_now-last_trigger_price|/last_trigger_price` 超过 `STRATEGY_VOL_THRESHOLD_BPS`（默认 80bps）且超过 `STRATEGY_VOL_WINDOW_SECS` 冷却窗口时，立即触发策略分析，并延后定时兜底的下一次执行。

运行机制要点：

1. **统一调度**：调度 loop 使用 `Notify` 同步波动事件与定时任务，只要有任意触发源准备就绪即可抢占 `ANALYSIS_PERMIT`。
2. **日志透明**：每次触发都会记录来源、现价、基准价、偏移及结果（成功/失败/忙），便于排查节奏。
3. **启动即基线**：在仅启用波动模式时，后端会在启动时为每个 symbol 跑一次分析并记录初始 `last_trigger_price`；若行情先到，则首个 ticker 会直接填充基线，确保波动触发能尽快生效。

> 推荐配置：将 `STRATEGY_SCHEDULE_INTERVAL_SECS` 设为 10~15 分钟，只保留兜底；波动触发阈值根据策略灵敏度自行在 40~120bps 间调节。

## 快速上手

1. **安装依赖**
   sudo apt install python3.12-venv
   sudo python3 -m venv venv && source venv/bin/activate

   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh  && source $HOME/.cargo/env

   sudo apt install -y postgresql postgresql-client
      sudo su - postgres
      psql
      ALTER USER aitrader_user WITH PASSWORD '123456';
      CREATE DATABASE aitrader OWNER aitrader_user;
      \c aitrader
      CREATE SCHEMA aitrader AUTHORIZATION aitrader_user;
      ALTER ROLE aitrader_user SET search_path TO aitrader, public;
      \q
      exit

   curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
   source $HOME/.bashrc  # 如果是zsh则用source $HOME/.zshrc

   nvm install 20
   nvm alias default 20

   curl -LsSf https://astral.sh/uv/install.sh | sh
   source /root/.cargo/env
   nano /root/.bashrc
   在文件末尾添加以下内容（复制粘贴）
   export PATH="/root/.cargo/bin:$PATH"

   
   - Rust stable、cargo、PostgreSQL。
   - Python 3.11+（建议使用 `uv` 或 `pip` 创建虚拟环境）。
   - Node.js 18+ 与 pnpm。
2. **准备环境变量**
   - 复制 `.env.example` 为 `.env`，补齐 `OKX_API_KEY/SECRET/PASSPHRASE`、`AGENT_BASE_URL`、`DATABASE_URL`、`DEEPSEEK_API_KEY` 等。
   - 按需调整 `STRATEGY_*` 参数（定时/波动/窗口）与 `OKX_INST_IDS`。
3. **启动服务**
   ```bash
   cd /home/ubuntu/Workspcace/AiTrader && source venv/bin/activate
   ```bash

   ```bash
   # Python Agent
   cd agent
   uv pip install -r requirements.txt
   cd ..
   uvicorn agent.llm.main:app --host 0.0.0.0 --port 8001
   # uvicorn llm.main:app --host 0.0.0.0 --port 8001
   ```bash

   ```bash
   cd ~/AiTrader/
   export PATH="$HOME/.local/bin:$PATH"
   export PYTHONPATH=$PWD:$PYTHONPATH
   uvicorn agent.llm.main:app --host 0.0.0.0 --port 8001
   ```bash

   ```bash
   # Rust API
   cd /home/ubuntu/Workspcace/AiTrader/backend
   export $(grep -v '^#' .env | xargs)
   cargo run
   ```bash 
   
   ```bash
   # React 前端
   cd frontend
   pnpm install
   pnpm dev --host
   ```

   ```bash
   VPN
   # 1. 打开clash代理
   clashon
   clashui
   http://101.35.245.230:9090/ui
   😼 当前密钥：NiqjpK
   进入web，选择美国节点

   Tumx
   tmux attach -t myagent
   tmux attach -t myagent1
   tmux attach -t myagent2

   
   # 2. 确保在当前窗口设置了代理
   export https_proxy=http://127.0.0.1:7890
   export http_proxy=http://127.0.0.1:7890
   export all_proxy=socks5://127.0.0.1:7890

   # 3. 再次验证（必须看到 OKX 的 HTML 输出才算通过）
   curl -I https://www.okx.com
   ```
4. **验证**
   - 打开前端查看账户/行情，并在“策略对话”中触发一次手动运行。
   - 观察 `backend/log/api-server.log` 中的触发日志，确认三种触发模式行为符合预期。

## 配置速览

- `OKX_INST_IDS`：需要跟踪/下单的合约列表（逗号分隔，默认 `BTC-USDT-SWAP`）。
- `STRATEGY_SCHEDULE_ENABLED` / `STRATEGY_SCHEDULE_INTERVAL_SECS`：定时触发开关与兜底周期（秒）。
- `STRATEGY_VOL_TRIGGER_ENABLED` / `STRATEGY_VOL_THRESHOLD_BPS` / `STRATEGY_VOL_WINDOW_SECS`：波动触发开关、阈值（bps）与冷却/观察窗口（秒）。
- `STRATEGY_MANUAL_TRIGGER_ENABLED`：前端是否显示手动触发按钮。
- `INITIAL_EQUITY` 与 `BALANCE_SNAPSHOT_*`：前端基线与账户快照写入阈值。
- `DATABASE_URL`、`RESET_DATABASE`：PostgreSQL 连接与重置策略。

更多变量可参考 `.env.example`。

## 部署

仓库提供 `nginx/build.sh` 用于一键打包：构建前端、后端并在 `agent/.venv` 安装依赖，产物可直接配合 systemd 与 nginx 部署。线上模式下建议：

- 为 Agent 与 API 设置独立 systemd service，确保重启顺序。
- 通过 `pm2`/`supervisord` 等守护 Python Agent，避免长时间推理导致进程退出。
- 配置 Grafana/Prometheus 或至少 tail `log/api-server.log`，关注策略触发日志与数据库同步状态。

### 构建 & 部署脚本

1. **构建产物**
   ```bash
   bash nginx/build.sh
   ```
   - 前提：已安装 `cargo`、`npm`、`python3`。
   - 行为：`cargo build --release`、`npm install && npm run build`、在 `agent/.venv` 安装依赖。
   - 产出：`backend/target/release/api-server`、`frontend/dist/`、`agent/.venv`。

2. **部署/运维**
   ```bash
   sudo bash nginx/deploy.sh deploy     # 首次部署（默认操作）
   sudo bash nginx/deploy.sh status     # 查看 systemd 状态
   sudo bash nginx/deploy.sh start|stop # 控制后台服务
   sudo bash nginx/deploy.sh uninstall  # 移除 nginx + systemd 配置
   ```
   - 依赖 `config/config.yaml`（可通过 `DEPLOY_CONFIG_FILE` 覆盖）描述域名、SSL、systemd 与静态文件路径。
   - 自动动作：校验二进制/前端产物 → 同步静态资源 → 写入 nginx 配置 → 创建/更新 backend & agent systemd unit → reload nginx。
   - 需要 root 权限运行；执行前请确保 SSL 证书、`config/config.yaml` 与 `OKX` 凭证已就绪。
