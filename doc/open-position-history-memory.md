# 开仓历史记忆

生成时间：2026-05-27 15:29:32 CST (+08:00)

## 结论

开仓/持仓历史不在 CSV 或普通日志里，主要保存在本机 PostgreSQL：

- 数据库：`aitrader`
- Schema：`aitrader`
- 数据源配置：项目根目录 `.env` 与 `backend/.env` 中的 `DATABASE_URL`、`DATABASE_SCHEMA`
- 主要表：
  - `aitrader.positions`：当前与历史持仓快照，最适合恢复开仓历史
  - `aitrader.orders`：订单历史
  - `aitrader.trades`：成交明细

当前数据库统计：

| 表 | 数量 |
| --- | ---: |
| `orders` | 156 |
| `trades` | 264 |
| `positions` | 43 |
| `strategies` | 734 |

## 读取规则

账户使用 `net` 持仓模式。`positions` 表中已平仓记录的 `size` 会变成 `0`，但 OKX 原始仓位数量保存在 `metadata.pos`。

因此读取历史开仓方向/规模时应使用：

```sql
COALESCE(NULLIF(size, 0), NULLIF((metadata->>'pos')::numeric, 0), size) AS restored_size
```

方向判断：

- `restored_size > 0`：long
- `restored_size < 0`：short
- `restored_size = 0`：flat

注意：`positions.last_trade_at` 是该仓位快照最近一次成交时间，不一定严格等于最初开仓时间；它是当前表结构里最接近“开仓/持仓历史时间点”的字段。逐笔开仓/平仓需要结合 `orders` 与 `trades` 重放净仓位。

## 当前未平仓

| snapshot_id | inst_id | direction | restored_size | avg_price | mark_px | unrealized_pnl | leverage | last_trade_at | updated_at |
| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| 4629 | BTC-USDT-SWAP | short | -12 | 76222.4 | 77591.1 | -164.244000000001 | 5 | 2026-05-01 07:01:37.197+08 | 2026-05-26 01:16:52.94152+08 |

## 历史快照摘要

- 总持仓快照：43
- 当前未平仓：1
- 已平仓：42
- 多头快照：25
- 空头快照：18
- 最早记录时间：2026-01-12 19:29:13.898+08
- 最新记录时间：2026-05-01 07:01:37.197+08

## 最近开仓/持仓历史

| snapshot_id | inst_id | direction | restored_size | avg_price | mark_px | unrealized_pnl | leverage | last_trade_at | closed_at |
| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| 4629 | BTC-USDT-SWAP | short | -12 | 76222.4 | 77591.1 | -164.244000000001 | 5 | 2026-05-01 07:01:37.197+08 |  |
| 4610 | BTC-USDT-SWAP | long | 2908.02 | 86610.9952706653 | 86244.4 | -10660.6637900003 | 20 | 2026-01-27 22:19:25.055+08 | 2026-01-27 22:38:38.44488+08 |
| 3521 | BTC-USDT-SWAP | short | -2885.18 | 90057.3 | 89945.1 | 3237.17195999992 | 20 | 2026-01-22 18:57:42.359+08 | 2026-01-23 13:12:49.341011+08 |
| 3475 | BTC-USDT-SWAP | short | -3906.07 | 94844 | 94917.6 | -2874.86752000023 | 20 | 2026-01-16 13:50:24.399+08 | 2026-01-16 14:36:40.963185+08 |
| 3302 | BTC-USDT-SWAP | long | 3022.36 | 95543.8999397822 | 95736.4 | 5818.04481999971 | 20 | 2026-01-15 22:44:04.399+08 | 2026-01-16 01:38:20.869306+08 |
| 3297 | BTC-USDT-SWAP | short | -2562.47 | 96208.4001151233 | 95716.4 | 12607.35535 | 20 | 2026-01-15 18:44:06.743+08 | 2026-01-15 18:49:47.798842+08 |
| 3284 | BTC-USDT-SWAP | long | 2269.42 | 95377.9947585727 | 95197.1 | -4105.26182999991 | 20 | 2026-01-15 16:49:48.054+08 | 2026-01-15 17:03:30.903199+08 |
| 3249 | BTC-USDT-SWAP | long | 2192.91 | 95361.2355614229 | 95484.9 | 2711.84983999996 | 20 | 2026-01-15 15:25:08.92+08 | 2026-01-15 16:00:19.537611+08 |
| 2290 | BTC-USDT-SWAP | long | 9.03 | 94500 | 95218.9 | 64.9166699999995 | 20 | 2026-01-14 17:18:07.258+08 | 2026-01-15 09:28:15.400191+08 |
| 2178 | BTC-USDT-SWAP | short | -2.34 | 94467.8 | 94461.1 | 0.156779999999932 | 5 | 2026-01-14 17:10:22.002+08 | 2026-01-14 17:12:41.050904+08 |
| 2163 | BTC-USDT-SWAP | long | 0.34 | 94078.3382352941 | 93918.9 | -0.542089999999999 | 5 | 2026-01-14 14:54:52.32+08 | 2026-01-14 15:08:09.472675+08 |
| 2086 | BTC-USDT-SWAP | short | -0.98 | 94587.3795918367 | 93935.7 | 6.38646000000008 | 5 | 2026-01-14 13:34:36.207+08 | 2026-01-14 14:48:06.321681+08 |

## 订单与成交摘要

订单汇总：

| inst_id | side | order_count | total_filled | first_order | latest_order |
| --- | --- | ---: | ---: | --- | --- |
| BTC-USDT-SWAP | buy | 80 | 263.23 | 2026-01-12 19:29:18.209557+08 | 2026-04-23 16:20:12.235655+08 |
| BTC-USDT-SWAP | sell | 76 | 268.92 | 2026-01-12 19:56:23.064235+08 | 2026-05-01 07:01:48.593011+08 |

成交汇总：

| inst_id | side | fill_count | total_filled | total_fee | realized_pnl | first_fill | latest_fill |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| BTC-USDT-SWAP | buy | 137 | 263.23 | -103.214776915 | -57.6791299999999712 | 2026-01-12 19:29:13.898+08 | 2026-04-23 16:20:02.214+08 |
| BTC-USDT-SWAP | sell | 127 | 268.92 | -105.988972660 | 857.9593643550809377 | 2026-01-12 19:56:10.315+08 | 2026-05-01 07:01:37.197+08 |

## 常用查询

先加载环境变量：

```bash
set -a
source .env
set +a
```

查看当前未平仓：

```bash
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -P pager=off -c "
SET search_path TO aitrader, public;
SELECT
  snapshot_id,
  inst_id,
  CASE
    WHEN COALESCE(NULLIF(size,0), NULLIF((metadata->>'pos')::numeric,0), size) > 0 THEN 'long'
    WHEN COALESCE(NULLIF(size,0), NULLIF((metadata->>'pos')::numeric,0), size) < 0 THEN 'short'
    ELSE 'flat'
  END AS direction,
  COALESCE(NULLIF(size,0), NULLIF((metadata->>'pos')::numeric,0), size) AS restored_size,
  avg_price,
  mark_px,
  unrealized_pnl,
  metadata->>'lever' AS leverage,
  last_trade_at,
  updated_at
FROM positions
WHERE closed_at IS NULL
ORDER BY updated_at DESC;
"
```

查看持仓历史：

```bash
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -P pager=off -c "
SET search_path TO aitrader, public;
SELECT
  snapshot_id,
  inst_id,
  CASE
    WHEN COALESCE(NULLIF(size,0), NULLIF((metadata->>'pos')::numeric,0), size) > 0 THEN 'long'
    WHEN COALESCE(NULLIF(size,0), NULLIF((metadata->>'pos')::numeric,0), size) < 0 THEN 'short'
    ELSE 'flat'
  END AS direction,
  COALESCE(NULLIF(size,0), NULLIF((metadata->>'pos')::numeric,0), size) AS restored_size,
  avg_price,
  mark_px,
  unrealized_pnl,
  metadata->>'lever' AS leverage,
  last_trade_at,
  closed_at,
  action_kind,
  updated_at
FROM positions
ORDER BY COALESCE(closed_at, updated_at) DESC;
"
```

