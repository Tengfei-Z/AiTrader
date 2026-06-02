import { Empty, Flex, Select, Slider, Spin, Switch, Tag, Tooltip, Typography } from 'antd';
import { useEffect, useMemo, useRef, useState } from 'react';
import type { MouseEvent, WheelEvent } from 'react';
import dayjs from 'dayjs';
import type { FillItem, Kline, OrderItem, PositionItem, StrategyMessage } from '@api/types';
import { useFills } from '@hooks/useFills';
import { useKlines } from '@hooks/useKlines';
import { useOpenOrders } from '@hooks/useOpenOrders';

type StrategyDirection = 'bullish' | 'bearish' | 'neutral';

interface StrategyMarker {
  id: string;
  candleIndex: number;
  time: number;
  direction: StrategyDirection;
  label: string;
  summary: string;
}

type PriceLevelKind = 'take-profit' | 'stop-loss' | 'order';

interface PriceLevel {
  id: string;
  price: number;
  kind: PriceLevelKind;
  label: string;
}

interface TradeMarker {
  id: string;
  candleIndex: number;
  time: number;
  price: number;
  side: 'buy' | 'sell';
  label: string;
}

interface Props {
  strategyMessages?: StrategyMessage[];
  positions?: PositionItem[];
  defaultSymbol?: string;
}

const SYMBOL_OPTIONS = ['BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'SOL-USDT-SWAP', 'BNB-USDT-SWAP', 'OKB-USDT-SWAP'];

const BAR_OPTIONS = [
  { label: '1m', value: '1m' },
  { label: '3m', value: '3m' },
  { label: '5m', value: '5m' },
  { label: '15m', value: '15m' },
  { label: '1H', value: '1H' },
  { label: '4H', value: '4H' }
];

const BAR_STORAGE_KEY = 'ai-trader.kline-bar';
const ZOOM_STORAGE_KEY = 'ai-trader.kline-zoom';
const MIN_ZOOM = 1;
const MAX_ZOOM = 4;
const ZOOM_STEP = 0.25;
const MIN_VISIBLE_CANDLES = 24;

const directionMeta: Record<StrategyDirection, { label: string; color: string }> = {
  bullish: { label: '看涨', color: '#16a34a' },
  bearish: { label: '看空', color: '#dc2626' },
  neutral: { label: '观望', color: '#64748b' }
};

const formatNumber = (value?: number) =>
  value !== undefined && Number.isFinite(value)
    ? value.toLocaleString(undefined, { maximumFractionDigits: 2 })
    : '--';

const parseNumericString = (value?: string) => {
  if (!value) {
    return undefined;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
};

const readMetadataNumber = (metadata: PositionItem['metadata'], key: string) => {
  if (!metadata || typeof metadata !== 'object' || Array.isArray(metadata)) {
    return undefined;
  }
  const raw = metadata[key];
  if (raw === null || raw === undefined) {
    return undefined;
  }
  if (typeof raw === 'number') {
    return Number.isFinite(raw) ? raw : undefined;
  }
  if (typeof raw === 'string') {
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : undefined;
  }
  return undefined;
};

const inferPositionDirection = (position: PositionItem) => {
  const metadataSize = readMetadataNumber(position.metadata, 'pos');
  const effectiveSize = Math.abs(position.size) > 1e-9 ? position.size : metadataSize;
  if (effectiveSize !== undefined) {
    if (effectiveSize > 0) {
      return 'long';
    }
    if (effectiveSize < 0) {
      return 'short';
    }
  }
  return position.side;
};

const readStoredBar = () => {
  const value = window.localStorage.getItem(BAR_STORAGE_KEY);
  return BAR_OPTIONS.some((option) => option.value === value) ? value : '3m';
};

const clampZoom = (value: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, value));

const readStoredZoom = () => {
  const raw = window.localStorage.getItem(ZOOM_STORAGE_KEY);
  const parsed = raw ? Number(raw) : MIN_ZOOM;
  return Number.isFinite(parsed) ? clampZoom(parsed) : MIN_ZOOM;
};

const resolveDirection = (summary: string): StrategyDirection => {
  const text = summary.toLowerCase();
  const bearish = ['看空', '做空', '开空', '空头', '卖出', '下跌', 'short', 'sell'].some((keyword) =>
    text.includes(keyword)
  );
  const bullish = ['看涨', '做多', '开多', '多头', '买入', '上涨', 'long', 'buy'].some((keyword) =>
    text.includes(keyword)
  );

  if (bullish && !bearish) {
    return 'bullish';
  }
  if (bearish && !bullish) {
    return 'bearish';
  }
  return 'neutral';
};

const nearestCandleIndex = (candles: Kline[], time: number) => {
  let bestIndex = -1;
  let bestDistance = Number.POSITIVE_INFINITY;

  candles.forEach((candle, index) => {
    const distance = Math.abs(candle.timestamp - time);
    if (distance < bestDistance) {
      bestDistance = distance;
      bestIndex = index;
    }
  });

  return bestIndex;
};

const buildMarkers = (candles: Kline[], messages?: StrategyMessage[]): StrategyMarker[] => {
  if (!messages?.length || !candles.length) {
    return [];
  }

  const firstTime = candles[0]?.timestamp ?? 0;
  const lastTime = candles[candles.length - 1]?.timestamp ?? 0;
  const windowPadding = Math.max(60_000, lastTime - firstTime);

  return messages
    .map((message) => {
      const time = new Date(message.createdAt).getTime();
      if (!Number.isFinite(time) || time < firstTime - windowPadding || time > lastTime + windowPadding) {
        return null;
      }

      const candleIndex = nearestCandleIndex(candles, time);
      if (candleIndex < 0) {
        return null;
      }

      const direction = resolveDirection(message.summary);
      return {
        id: message.id,
        candleIndex,
        time,
        direction,
        label: directionMeta[direction].label,
        summary: message.summary
      };
    })
    .filter((marker): marker is StrategyMarker => marker !== null)
    .slice(0, 8);
};

const inferLevelKind = (price: number, position?: PositionItem): PriceLevelKind => {
  if (!position?.avgPrice) {
    return 'order';
  }

  const direction = inferPositionDirection(position);
  if (direction === 'long') {
    return price >= position.avgPrice ? 'take-profit' : 'stop-loss';
  }
  if (direction === 'short') {
    return price <= position.avgPrice ? 'take-profit' : 'stop-loss';
  }
  return 'order';
};

const levelMeta: Record<PriceLevelKind, { label: string; color: string }> = {
  'take-profit': { label: '止盈', color: '#16a34a' },
  'stop-loss': { label: '止损', color: '#dc2626' },
  order: { label: '挂单', color: '#2563eb' }
};

const buildPriceLevels = (
  symbol: string,
  positions?: PositionItem[],
  openOrders?: OrderItem[]
): PriceLevel[] => {
  const activePosition = positions?.find((position) => position.instId === symbol && Math.abs(position.size) > 1e-9);
  const levels: PriceLevel[] = [];

  if (activePosition?.takeProfitPx) {
    levels.push({
      id: `${symbol}-position-tp`,
      price: activePosition.takeProfitPx,
      kind: 'take-profit',
      label: '止盈'
    });
  }

  if (activePosition?.stopLossPx) {
    levels.push({
      id: `${symbol}-position-sl`,
      price: activePosition.stopLossPx,
      kind: 'stop-loss',
      label: '止损'
    });
  }

  openOrders
    ?.filter((order) => order.symbol === symbol)
    .forEach((order) => {
      const explicitLevels = [
        { value: parseNumericString(order.takeProfitPx), kind: 'take-profit' as PriceLevelKind },
        { value: parseNumericString(order.stopLossPx), kind: 'stop-loss' as PriceLevelKind }
      ].filter((entry): entry is { value: number; kind: PriceLevelKind } => entry.value !== undefined);

      const fallbackPx = parseNumericString(order.triggerPx);
      const inferredLevels = explicitLevels.length
        ? explicitLevels
        : fallbackPx !== undefined
          ? [{ value: fallbackPx, kind: inferLevelKind(fallbackPx, activePosition) }]
          : [];

      inferredLevels.forEach((entry) => {
        const meta = levelMeta[entry.kind];
        levels.push({
          id: `${order.orderId}-${entry.kind}-${entry.value}`,
          price: entry.value,
          kind: entry.kind,
          label: meta.label
        });
      });
    });

  const seen = new Set<string>();
  return levels.filter((level) => {
    const key = `${level.kind}-${level.price.toFixed(8)}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
};

const buildTradeMarkers = (candles: Kline[], fills?: FillItem[]): TradeMarker[] => {
  if (!fills?.length || !candles.length) {
    return [];
  }

  const firstTime = candles[0]?.timestamp ?? 0;
  const lastTime = candles[candles.length - 1]?.timestamp ?? 0;
  const windowPadding = Math.max(60_000, lastTime - firstTime);

  return fills
    .map((fill) => {
      const time = Number(fill.timestamp);
      const price = Number(fill.price);
      if (
        !Number.isFinite(time) ||
        !Number.isFinite(price) ||
        time < firstTime - windowPadding ||
        time > lastTime + windowPadding
      ) {
        return null;
      }

      const candleIndex = nearestCandleIndex(candles, time);
      if (candleIndex < 0) {
        return null;
      }

      return {
        id: fill.fillId,
        candleIndex,
        time,
        price,
        side: fill.side,
        label: fill.side === 'buy' ? '买' : '卖'
      };
    })
    .filter((marker): marker is TradeMarker => marker !== null)
    .slice(0, 80);
};

const drawRoundRect = (
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number
) => {
  const r = Math.min(radius, width / 2, height / 2);
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + width, y, x + width, y + height, r);
  ctx.arcTo(x + width, y + height, x, y + height, r);
  ctx.arcTo(x, y + height, x, y, r);
  ctx.arcTo(x, y, x + width, y, r);
  ctx.closePath();
};

const drawChart = (
  canvas: HTMLCanvasElement,
  candles: Kline[],
  markers: StrategyMarker[],
  priceLevels: PriceLevel[],
  tradeMarkers: TradeMarker[],
  hoveredIndex?: number
) => {
  const parent = canvas.parentElement;
  const width = parent?.clientWidth ?? 0;
  const height = parent?.clientHeight ?? 0;
  const dpr = window.devicePixelRatio || 1;
  if (!width || !height) {
    return;
  }

  canvas.width = Math.floor(width * dpr);
  canvas.height = Math.floor(height * dpr);
  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;

  const ctx = canvas.getContext('2d');
  if (!ctx) {
    return;
  }

  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  ctx.fillStyle = '#ffffff';
  ctx.fillRect(0, 0, width, height);

  if (!candles.length) {
    return;
  }

  const padding = { top: 22, right: 22, bottom: 36, left: 68 };
  const volumeHeight = Math.min(86, Math.max(56, height * 0.18));
  const volumeGap = 18;
  const chartTop = padding.top;
  const chartBottom = height - padding.bottom - volumeHeight - volumeGap;
  const chartHeight = chartBottom - chartTop;
  const chartLeft = padding.left;
  const chartRight = width - padding.right;
  const chartWidth = chartRight - chartLeft;
  const volumeTop = chartBottom + volumeGap;
  const volumeBottom = height - padding.bottom;

  const lows = [...candles.map((candle) => candle.low), ...priceLevels.map((level) => level.price), ...tradeMarkers.map((marker) => marker.price)];
  const highs = [...candles.map((candle) => candle.high), ...priceLevels.map((level) => level.price), ...tradeMarkers.map((marker) => marker.price)];
  const minPrice = Math.min(...lows);
  const maxPrice = Math.max(...highs);
  const pricePadding = Math.max((maxPrice - minPrice) * 0.08, maxPrice * 0.001);
  const lowBound = minPrice - pricePadding;
  const highBound = maxPrice + pricePadding;
  const priceRange = highBound - lowBound || 1;
  const volumeMax = Math.max(...candles.map((candle) => candle.volume), 1);
  const step = chartWidth / Math.max(candles.length, 1);
  const candleWidth = Math.min(12, Math.max(3, step * 0.58));

  const xFor = (index: number) => chartLeft + step * index + step / 2;
  const yFor = (price: number) => chartBottom - ((price - lowBound) / priceRange) * chartHeight;

  ctx.strokeStyle = '#e5e7eb';
  ctx.lineWidth = 1;
  ctx.fillStyle = '#94a3b8';
  ctx.font = '12px Inter, sans-serif';
  ctx.textBaseline = 'middle';

  for (let i = 0; i <= 4; i += 1) {
    const y = chartTop + (chartHeight / 4) * i;
    const price = highBound - (priceRange / 4) * i;
    ctx.beginPath();
    ctx.moveTo(chartLeft, y);
    ctx.lineTo(chartRight, y);
    ctx.stroke();
    ctx.textAlign = 'right';
    ctx.fillText(formatNumber(price), chartLeft - 8, y);
  }

  const timeTicks = 5;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'top';
  for (let i = 0; i < timeTicks; i += 1) {
    const index = Math.round((candles.length - 1) * (i / Math.max(timeTicks - 1, 1)));
    const x = xFor(index);
    ctx.fillText(dayjs(candles[index].timestamp).format('MM-DD HH:mm'), x, height - padding.bottom + 10);
  }

  candles.forEach((candle, index) => {
    const x = xFor(index);
    const up = candle.close >= candle.open;
    const color = up ? '#16a34a' : '#dc2626';
    const highY = yFor(candle.high);
    const lowY = yFor(candle.low);
    const openY = yFor(candle.open);
    const closeY = yFor(candle.close);
    const bodyTop = Math.min(openY, closeY);
    const bodyHeight = Math.max(Math.abs(closeY - openY), 2);
    const volumeBarHeight = (candle.volume / volumeMax) * (volumeBottom - volumeTop);

    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    ctx.moveTo(x, highY);
    ctx.lineTo(x, lowY);
    ctx.stroke();
    ctx.globalAlpha = up ? 0.9 : 0.82;
    ctx.fillRect(x - candleWidth / 2, bodyTop, candleWidth, bodyHeight);
    ctx.globalAlpha = 0.18;
    ctx.fillRect(x - candleWidth / 2, volumeBottom - volumeBarHeight, candleWidth, volumeBarHeight);
    ctx.globalAlpha = 1;
  });

  priceLevels.forEach((level, index) => {
    const meta = levelMeta[level.kind];
    const y = yFor(level.price);
    ctx.strokeStyle = meta.color;
    ctx.fillStyle = meta.color;
    ctx.lineWidth = 1.4;
    ctx.setLineDash([6, 4]);
    ctx.beginPath();
    ctx.moveTo(chartLeft, y);
    ctx.lineTo(chartRight, y);
    ctx.stroke();
    ctx.setLineDash([]);

    const label = `${level.label} ${formatNumber(level.price)}`;
    ctx.font = '12px Inter, sans-serif';
    const labelWidth = ctx.measureText(label).width + 14;
    const labelX = Math.max(chartLeft + 4, chartRight - labelWidth - 4);
    const labelY = Math.min(Math.max(y - 10 + (index % 2) * 18, chartTop), chartBottom - 20);
    drawRoundRect(ctx, labelX, labelY, labelWidth, 20, 4);
    ctx.globalAlpha = 0.9;
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.fillStyle = '#ffffff';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(label, labelX + labelWidth / 2, labelY + 10);
  });

  tradeMarkers.forEach((marker, markerIndex) => {
    const x = xFor(marker.candleIndex);
    const y = yFor(marker.price);
    const color = marker.side === 'buy' ? '#16a34a' : '#dc2626';
    const yOffset = markerIndex % 2 === 0 ? -14 : 14;

    ctx.fillStyle = color;
    ctx.strokeStyle = '#ffffff';
    ctx.lineWidth = 2;
    ctx.beginPath();
    if (marker.side === 'buy') {
      ctx.moveTo(x, y + yOffset - 8);
      ctx.lineTo(x - 6, y + yOffset + 5);
      ctx.lineTo(x + 6, y + yOffset + 5);
    } else {
      ctx.moveTo(x, y + yOffset + 8);
      ctx.lineTo(x - 6, y + yOffset - 5);
      ctx.lineTo(x + 6, y + yOffset - 5);
    }
    ctx.closePath();
    ctx.fill();
    ctx.stroke();

    ctx.font = '11px Inter, sans-serif';
    ctx.fillStyle = '#0f172a';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(marker.label, x, y + yOffset + (marker.side === 'buy' ? 15 : -15));
  });

  if (hoveredIndex !== undefined && hoveredIndex >= 0 && hoveredIndex < candles.length) {
    const x = xFor(hoveredIndex);
    ctx.strokeStyle = '#64748b';
    ctx.setLineDash([4, 4]);
    ctx.beginPath();
    ctx.moveTo(x, chartTop);
    ctx.lineTo(x, volumeBottom);
    ctx.stroke();
    ctx.setLineDash([]);
  }

  markers.forEach((marker, markerIndex) => {
    const candle = candles[marker.candleIndex];
    if (!candle) {
      return;
    }

    const meta = directionMeta[marker.direction];
    const x = xFor(marker.candleIndex);
    const baseY =
      marker.direction === 'bullish'
        ? Math.min(yFor(candle.low) + 22, chartBottom - 10)
        : Math.max(yFor(candle.high) - 22, chartTop + 10);
    const y = baseY + (markerIndex % 2 === 0 ? 0 : marker.direction === 'bullish' ? 12 : -12);

    ctx.fillStyle = meta.color;
    ctx.strokeStyle = '#ffffff';
    ctx.lineWidth = 2;
    ctx.beginPath();
    if (marker.direction === 'bullish') {
      ctx.moveTo(x, y - 9);
      ctx.lineTo(x - 7, y + 5);
      ctx.lineTo(x + 7, y + 5);
    } else if (marker.direction === 'bearish') {
      ctx.moveTo(x, y + 9);
      ctx.lineTo(x - 7, y - 5);
      ctx.lineTo(x + 7, y - 5);
    } else {
      ctx.arc(x, y, 6, 0, Math.PI * 2);
    }
    ctx.closePath();
    ctx.fill();
    ctx.stroke();

    const label = marker.label;
    ctx.font = '12px Inter, sans-serif';
    const labelWidth = ctx.measureText(label).width + 14;
    const labelX = Math.min(Math.max(x - labelWidth / 2, chartLeft), chartRight - labelWidth);
    const labelY = marker.direction === 'bullish' ? y + 10 : y - 28;
    drawRoundRect(ctx, labelX, labelY, labelWidth, 20, 4);
    ctx.fillStyle = '#0f172a';
    ctx.globalAlpha = 0.86;
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.fillStyle = '#ffffff';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(label, labelX + labelWidth / 2, labelY + 10);
  });
};

const RealtimeKlineChart = ({ strategyMessages, positions, defaultSymbol }: Props) => {
  const [symbol, setSymbol] = useState(defaultSymbol ?? SYMBOL_OPTIONS[0]);
  const [bar, setBar] = useState(readStoredBar);
  const [zoom, setZoom] = useState(readStoredZoom);
  const [live, setLive] = useState(true);
  const [hoveredIndex, setHoveredIndex] = useState<number>();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const symbolOptions = useMemo(
    () => Array.from(new Set([defaultSymbol, ...SYMBOL_OPTIONS].filter(Boolean))) as string[],
    [defaultSymbol]
  );

  useEffect(() => {
    if (defaultSymbol) {
      setSymbol(defaultSymbol);
    }
  }, [defaultSymbol]);

  const { data, isFetching, isLoading } = useKlines({ symbol, bar, limit: 240 }, true, live);
  const { data: fills } = useFills(symbol, 100, Boolean(symbol));
  const { data: openOrders } = useOpenOrders(symbol, Boolean(symbol));
  const candles = data?.candles ?? [];
  const visibleCount = Math.max(MIN_VISIBLE_CANDLES, Math.round(candles.length / zoom));
  const visibleCandles = useMemo(() => candles.slice(-visibleCount), [candles, visibleCount]);
  const latest = candles[candles.length - 1];
  const markers = useMemo(() => buildMarkers(visibleCandles, strategyMessages), [visibleCandles, strategyMessages]);
  const priceLevels = useMemo(() => buildPriceLevels(symbol, positions, openOrders), [symbol, positions, openOrders]);
  const tradeMarkers = useMemo(() => buildTradeMarkers(visibleCandles, fills), [visibleCandles, fills]);
  const hoveredCandle = hoveredIndex !== undefined ? visibleCandles[hoveredIndex] : undefined;
  const latestDirection = latest ? latest.close - latest.open : 0;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return undefined;
    }

    const render = () => drawChart(canvas, visibleCandles, markers, priceLevels, tradeMarkers, hoveredIndex);
    render();

    const observer = new ResizeObserver(render);
    if (canvas.parentElement) {
      observer.observe(canvas.parentElement);
    }

    return () => observer.disconnect();
  }, [visibleCandles, markers, priceLevels, tradeMarkers, hoveredIndex]);

  useEffect(() => {
    setHoveredIndex(undefined);
  }, [symbol, bar, zoom]);

  const handleMouseMove = (event: MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas || !visibleCandles.length) {
      setHoveredIndex(undefined);
      return;
    }

    const rect = canvas.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const chartLeft = 68;
    const chartRight = rect.width - 22;
    if (x < chartLeft || x > chartRight) {
      setHoveredIndex(undefined);
      return;
    }

    const step = (chartRight - chartLeft) / visibleCandles.length;
    const index = Math.min(visibleCandles.length - 1, Math.max(0, Math.floor((x - chartLeft) / step)));
    setHoveredIndex(index);
  };

  const setStoredZoom = (value: number) => {
    const next = clampZoom(value);
    setZoom(next);
    window.localStorage.setItem(ZOOM_STORAGE_KEY, String(next));
  };

  const handleZoomChange = (value: number | number[]) => {
    const next = Array.isArray(value) ? value[0] : value;
    setStoredZoom(next);
  };

  const handleWheel = (event: WheelEvent<HTMLCanvasElement>) => {
    event.preventDefault();
    const direction = event.deltaY < 0 ? 1 : -1;
    setStoredZoom(zoom + direction * ZOOM_STEP);
  };

  const handleBarChange = (value: string) => {
    setBar(value);
    window.localStorage.setItem(BAR_STORAGE_KEY, value);
  };

  return (
    <Flex vertical gap={14} className="kline-panel">
      <Flex align="center" justify="space-between" gap={12} className="kline-toolbar">
        <Flex align="center" gap={10} wrap="wrap">
          <Select
            value={symbol}
            onChange={setSymbol}
            options={symbolOptions.map((value) => ({ label: value.replace('-SWAP', ''), value }))}
            className="kline-symbol-select"
          />
          <Select value={bar} onChange={handleBarChange} options={BAR_OPTIONS} className="kline-bar-select" />
          <Tooltip title="开启后每 15 秒刷新一次最新 K 线">
            <Switch checked={live} checkedChildren="实时" unCheckedChildren="暂停" onChange={setLive} />
          </Tooltip>
          <Flex align="center" gap={8} className="kline-zoom-control">
            <Typography.Text type="secondary">缩放</Typography.Text>
            <Slider
              min={MIN_ZOOM}
              max={MAX_ZOOM}
              step={ZOOM_STEP}
              value={zoom}
              onChange={handleZoomChange}
              style={{ width: 118 }}
            />
          </Flex>
        </Flex>
        <Flex align="center" gap={8} className="kline-latest">
          <Typography.Text type="secondary">最新价</Typography.Text>
          <Typography.Text className={latestDirection >= 0 ? 'positive' : 'negative'}>
            {formatNumber(latest?.close)}
          </Typography.Text>
          {isFetching && <Typography.Text type="secondary">刷新中</Typography.Text>}
        </Flex>
      </Flex>

      <div className="kline-canvas-wrap">
        <Spin spinning={isLoading}>
          {visibleCandles.length ? (
            <>
              <canvas
                ref={canvasRef}
                className="kline-canvas"
                onMouseMove={handleMouseMove}
                onWheel={handleWheel}
                onMouseLeave={() => setHoveredIndex(undefined)}
              />
              {hoveredCandle && (
                <div className="kline-hover-card">
                  <Typography.Text strong>{dayjs(hoveredCandle.timestamp).format('MM-DD HH:mm')}</Typography.Text>
                  <span>开 {formatNumber(hoveredCandle.open)}</span>
                  <span>高 {formatNumber(hoveredCandle.high)}</span>
                  <span>低 {formatNumber(hoveredCandle.low)}</span>
                  <span>收 {formatNumber(hoveredCandle.close)}</span>
                </div>
              )}
            </>
          ) : (
            <Empty description="暂无 K 线数据" image={Empty.PRESENTED_IMAGE_SIMPLE} />
          )}
        </Spin>
      </div>

      <Flex align="center" gap={8} wrap="wrap" className="kline-marker-strip">
        <Typography.Text type="secondary">价位</Typography.Text>
        {priceLevels.length ? (
          priceLevels.map((level) => (
            <Tag key={level.id} color={levelMeta[level.kind].color}>
              {level.label} {formatNumber(level.price)}
            </Tag>
          ))
        ) : (
          <Typography.Text type="secondary">暂无止盈止损挂单</Typography.Text>
        )}
        <Typography.Text type="secondary">成交</Typography.Text>
        {tradeMarkers.length ? (
          <Typography.Text type="secondary">{tradeMarkers.length} 笔已标注</Typography.Text>
        ) : (
          <Typography.Text type="secondary">暂无可匹配成交</Typography.Text>
        )}
        <Typography.Text type="secondary">AI标注</Typography.Text>
        {markers.length ? (
          markers.map((marker) => (
            <Tooltip key={marker.id} title={marker.summary}>
              <Tag color={directionMeta[marker.direction].color}>
                {dayjs(marker.time).format('HH:mm')} {marker.label}
              </Tag>
            </Tooltip>
          ))
        ) : (
          <Typography.Text type="secondary">暂无可匹配到当前窗口的策略标注</Typography.Text>
        )}
      </Flex>
    </Flex>
  );
};

export default RealtimeKlineChart;
