import type { FillItem } from '@api/types';

export interface EquityPoint {
  time: number;
  equity: number;
  label?: string;
}

interface EquityMetrics {
  points: EquityPoint[];
  totalPnl?: number;
  pnl24h?: number;
  netPosition: number;
  avgEntryPrice?: number;
  unrealizedPnl?: number;
}

const safelyToNumber = (value?: string) => {
  if (!value) return 0;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
};

const DEFAULT_INITIAL_EQUITY = 131_939.73;

export const buildEquityCurve = (
  fills?: FillItem[],
  markPrice?: number,
  initialEquity = DEFAULT_INITIAL_EQUITY
): EquityMetrics => {
  const now = Date.now();

  if (!fills || fills.length === 0) {
    return {
      points: [
        {
          time: now,
          equity: initialEquity,
          label: '现在'
        }
      ],
      totalPnl: 0,
      pnl24h: 0,
      netPosition: 0,
      avgEntryPrice: undefined,
      unrealizedPnl: undefined
    };
  }

  const sorted = [...fills].sort((a, b) => Number(a.timestamp) - Number(b.timestamp));

  let position = 0;
  let cash = 0;
  let cost = 0;
  let avgEntry: number | undefined;
  const points: EquityPoint[] = [];

  sorted.forEach((fill) => {
    const size = safelyToNumber(fill.size);
    const price = safelyToNumber(fill.price);
    const fee = safelyToNumber(fill.fee);
    const tradeSign = fill.side === 'buy' ? 1 : -1;
    let signedSize = tradeSign * size;

    if (position !== 0 && Math.sign(position) !== tradeSign) {
      const quantityToClose = Math.min(Math.abs(position), Math.abs(signedSize));
      const prevSign = Math.sign(position);
      position += tradeSign * quantityToClose;
      if (avgEntry !== undefined && quantityToClose > 0) {
        cost -= avgEntry * prevSign * quantityToClose;
      }
      signedSize -= tradeSign * quantityToClose;
      if (position === 0) {
        avgEntry = undefined;
        cost = 0;
      } else if (avgEntry !== undefined) {
        avgEntry = cost / position;
      }
    }

    if (signedSize !== 0) {
      cost += price * signedSize;
      position += signedSize;
      avgEntry = position !== 0 ? cost / position : undefined;
    }

    if (fill.side === 'buy') {
      cash -= price * size + fee;
    } else {
      cash += price * size - fee;
    }

    const equity = initialEquity + cash + position * price;
    points.push({
      time: Number(fill.timestamp),
      equity
    });
  });

  const lastPoint = points[points.length - 1];
  const referencePrice = markPrice ?? safelyToNumber(sorted[sorted.length - 1].price);
  const finalEquity = initialEquity + cash + position * referencePrice;

  if (!lastPoint || Math.abs(now - lastPoint.time) > 60 * 1000) {
    points.push({
      time: now,
      equity: finalEquity,
      label: '现在'
    });
  } else {
    lastPoint.equity = finalEquity;
    lastPoint.label = '现在';
  }

  const firstEquity = points[0]?.equity ?? initialEquity;
  const totalPnl = finalEquity - firstEquity;

  const dayAgo = now - 24 * 60 * 60 * 1000;
  const pivot = [...points].reverse().find((point) => point.time <= dayAgo);
  const pnl24h = pivot ? finalEquity - pivot.equity : undefined;

  const unrealizedPnl =
    referencePrice !== undefined && avgEntry !== undefined ? (referencePrice - avgEntry) * position : undefined;

  return {
    points,
    totalPnl,
    pnl24h,
    netPosition: position,
    avgEntryPrice: avgEntry,
    unrealizedPnl
  };
};
