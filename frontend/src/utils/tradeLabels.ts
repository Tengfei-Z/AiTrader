import type { FillItem, OrderItem } from '@api/types';

export const formatTradeSideLabel = (side: 'buy' | 'sell' | string, reduceOnly?: boolean) => {
  if (reduceOnly) {
    return side === 'buy' ? '平空' : '平多';
  }
  return side === 'buy' ? '开多' : '开空';
};

export const tradeSideColor = (side: 'buy' | 'sell' | string, reduceOnly?: boolean) => {
  if (reduceOnly) {
    return 'blue';
  }
  return side === 'buy' ? 'green' : 'volcano';
};

const hasRealizedPnl = (fill: FillItem) => {
  if (fill.pnl === undefined || fill.pnl === null || fill.pnl === '') {
    return false;
  }
  return Number.isFinite(Number(fill.pnl));
};

const inferFillReduceOnly = (fill: FillItem, orders?: OrderItem[]) => {
  const order = orders?.find((item) => item.orderId === fill.orderId);
  return Boolean(order?.reduceOnly || order?.closeOnTrigger || hasRealizedPnl(fill));
};

export const inferFillActionLabel = (fill: FillItem, orders?: OrderItem[]) => {
  return formatTradeSideLabel(fill.side, inferFillReduceOnly(fill, orders));
};

export const inferFillActionColor = (fill: FillItem, orders?: OrderItem[]) => {
  return tradeSideColor(fill.side, inferFillReduceOnly(fill, orders));
};
