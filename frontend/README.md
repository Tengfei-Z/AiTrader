# AiTrader 前端

React + TypeScript + Vite 构建的控制台界面，提供行情展示、交易下单与账户信息查看。

## 功能概览

- **行情概览**：最新价、涨跌趋势、24h 指标、盘口深度、最新成交。
- **交易下单**：限价/市价下单、委托管理、快速撤单。
- **账户中心**：资产余额、历史订单、成交记录。
- **全局交易对切换**：侧栏导航 + 顶部选择器统一控制页面数据。

## 启动步骤

```bash
cd frontend
npm install
npm run dev
```

开发服务器默认监听 `http://localhost:5173`，并通过 Vite 代理把 `/api` 请求转发到 `http://localhost:3000`。如需修改，可在 `vite.config.ts` 中调整。

## 环境要求

- Node.js >= 18
- 配套后端 API (`/api/market`, `/api/account`)，具体协议见 `doc/API.md`。

## 代码结构

```
src/
  api/          // axios 实例与 REST 封装
  components/   // 可复用 UI 组件（行情卡、盘口、订单表单等）
  hooks/        // React Query 数据查询 hooks
  pages/        // Dashboard / Trade / Account 页面
  store/        // Zustand 全局状态（交易对）
  utils/        // 工具函数（预留）
```

## 下一步建议

- 接入 WebSocket，实现行情与委托的实时推送。
- 增加登录鉴权、用户权限控制。
- 引入国际化、主题等扩展特性（在 MVP 验证后再逐步推进）。
