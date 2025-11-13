export interface ApiResponse<T> {
  success: boolean;
  data: T;
  error: string | null;
}

export interface Ticker {
  symbol: string;
  last: string;
  bidPx?: string;
  askPx?: string;
  high24h?: string;
  low24h?: string;
  vol24h?: string;
  timestamp: string;
}

export interface OrderBook {
  bids: [string, string][];
  asks: [string, string][];
  timestamp: string;
}

export interface TradeItem {
  tradeId: string;
  price: string;
  size: string;
  side: 'buy' | 'sell';
  timestamp: string;
}

export interface BalanceItem {
  asset: string;
  available: string;
  locked: string;
  valuationUSDT?: string;
}

export interface BalanceSnapshotItem {
  asset: string;
  available: string;
  locked: string;
  valuation: string;
  source: string;
  recordedAt: string;
}

export type JsonMap = Record<string, unknown>;

export interface PositionSnapshot {
  instId: string;
  posSide: string;
  tdMode?: string;
  side: string;
  size: number;
  avgPrice?: number;
  markPx?: number;
  margin?: number;
  unrealizedPnl?: number;
  lastTradeAt?: string;
  closedAt?: string;
  actionKind?: string;
  entryOrdId?: string;
  exitOrdId?: string;
  metadata?: JsonMap;
  updatedAt: string;
}

export type PositionItem = PositionSnapshot;

export type PositionHistoryItem = PositionSnapshot;

export interface StrategyMessage {
  id: string;
  sessionId: string;
  summary: string;
  createdAt: string;
}

export interface InitialEquityRecord {
  amount: string;
  recordedAt: string;
}

export type OrderStatus = 'open' | 'partially_filled' | 'filled' | 'canceled';

export interface OrderItem {
  orderId: string;
  symbol: string;
  side: 'buy' | 'sell';
  type: 'limit' | 'market';
  price?: string;
  size: string;
  filledSize: string;
  status: OrderStatus;
  createdAt: string;
}

export interface FillItem {
  fillId: string;
  orderId: string;
  symbol: string;
  side: 'buy' | 'sell';
  price: string;
  size: string;
  fee: string;
  pnl?: string;
  timestamp: string;
}
