## 持仓与收益信息同步方案

本文档为将期货合约的**实时持仓**、**历史持仓**、**收益数据**等信息从交易所 API 同步到自身系统，并提供给前端统一展示的业务流程描述。

### 1. 总体架构

- **交易所 API**：后端定时或按需调用 OKX 的 REST/WebSocket 接口拉取持仓、订单、成交、收益等数据。
- **后端 agent**：负责统一请求、鉴权、限流、数据映射，并把数据写入自身数据库或缓存；同时向前端/内网服务提供 API。
- **数据库 + 缓存**：基础合约信息、持仓/订单快照、盈亏记录等写入 DB，热数据可再加 Redis 提升读取性能。
- **前端**：只跟本系统的后端交互，不再直接调用 OKX，确保安全、审计和一致性。

### 2. 关键数据源 & OKX 接口参考

| 目标 | OKX 接口 |
| --- | --- |
| 当前持仓 | `GET /api/v5/account/positions`（`position`） |
| 可用资产/保证金 | `GET /api/v5/account/balance` |
| 已平仓盈亏 | `GET /api/v5/account/trade/closed-pnl` |
| 实时订单/成交 | `GET /api/v5/trade/orders-pending`、`GET /api/v5/trade/fills` |
| 行情/合约参数 | `GET /api/v5/public/instruments`、`GET /api/v5/public/open-interest`等 |

### 3. 同步流程建议

1. **基础合约信息同步（周期性）**  
   agent 以日/小时级别调用 `instruments` 等接口，更新 `symbol`、`margin`, `fee` 等字段到本地 `contract` 表，为后续计算提供上下文。

2. **实时持仓同步（频繁）**  
   - 轮询或通过 WebSocket 监听 `positions`；同步后写入 `current_positions` 表/redis，并更新 `contracts`+`balances` 关联。
   - 记得同步 `posSide`、`avgPx`、`unrealisedPnl`、`liqPx` 等字段，前端展示时可以直接读取。

3. **历史持仓与盈亏（落地、归档）**  
   - 每笔成交可从 `trade/fills` + `closed-pnl` 组合计算；建议在交易完成后（成交确认、资金结算）写入 `position_history`、`pnl_records` 表。  
   - `closed-pnl` 本身就提供 `instId` 级别的平均盈亏，可用作查询某个合约的历史收益。

4. **策略/用户收益查询**  
   - 提供后端接口 `GET /internal/positions/{userId}`、`/profit/{instId}` 从 DB 聚合 `position_history`／`pnl_records`，并结合 `current_positions` 提供完整视图；  
   - 为避免频繁调用 OKX 可设置缓存，缓存策略可基于 `userId`、`instId` 及时间窗口（如 10s）刷新。

5. **事件驱动推送（可选）**  
   - 每次主动下单/成交，将事件写入消息队列（如 PQ），由消费方补写 DB、触发 cache 失效、推送 WebSocket 通知前端等。

6. **落地与审计**  
   - 所有 OKX 返回的成交/盈亏数据都保留原始响应字段，附带时间戳、请求 ID，便于追溯。  
   - 定期用 OKX 的 `closed-pnl`/`fills` 同步校验 DB 中的 `pnl_records` 是否一致。

### 4. 表结构（示例）

- `contracts(symbol, margin_rate, fee_rate, min_size, max_size, last_sync)`  
- `current_positions(user_id, inst_id, pos_side, size, avg_price, unrealized_pnl, margin, update_at)`  
- `position_history(user_id, inst_id, direction, size, open_price, close_price, realized_pnl, close_at)`  
- `pnl_records(user_id, inst_id, inst_name, pnl, pnl_time, source)`  

### 5. 前端展示逻辑

1. `current_positions` 返回当前持仓列表，可实时展示仓位、保证金、未实现盈亏等字段。
2. 历史持仓/盈亏通过 `position_history` 和 `pnl_records` 聚合后端接口获取（分页/筛选），避免前端自行调用 OKX。
3. 特殊收益查询可以按 `instId`/时间范围过滤 `pnl_records`；必要时也可以直接暴露 OKX `closed-pnl` 原始数据总览。

### 6. 其他注意事项

- 所有对 OKX 的请求都需签名，agent 统一封装鉴权逻辑，防止前端暴露密钥。  
- 由于盈亏计算敏感，建议在同步时记录请求响应、校验结果，确保和 OKX 账本一致。  
- 大量历史数据建议使用分页/分批写入，避免单次同步阻塞下游服务。

如需，我可以再补充一份具体的 API 调用顺序流程图或数据库迁移脚本。当前文档主要从「数据来源 → 写入 → 前端展现」的链路进行说明。 
