export interface ApiResponse<T> {
  success: boolean;
  data: T;
  error: string | null;
}

export interface Ticker {
  symbol: string;
  bar: string;
  last: string;
  open24h?: string;
  bidPx?: string;
  askPx?: string;
  high24h?: string;
  low24h?: string;
  vol24h?: string;
  volCcy24h?: string;
  timestamp: string;
}

export interface Kline {
  timestamp: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
  volumeCcy: number;
}

export interface KlinePayload {
  symbol: string;
  bar: string;
  candles: Kline[];
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

export interface BalanceSnapshotListPayload {
  snapshots: BalanceSnapshotItem[];
  hasMore: boolean;
  nextCursor?: string | null;
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
  takeProfitPx?: number;
  stopLossPx?: number;
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

export interface ModelBudget {
  remaining: number;
  limit: number;
  used: number;
}

export interface StrategyChatPayload {
  allowManualTrigger: boolean;
  modelBudget?: ModelBudget;
  messages: StrategyMessage[];
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
  posSide?: string;
  orderType: string;
  type?: 'limit' | 'market';
  price?: string;
  triggerPx?: string;
  takeProfitPx?: string;
  stopLossPx?: string;
  size: string;
  filledSize: string;
  status: OrderStatus | string;
  reduceOnly?: boolean;
  closeOnTrigger?: boolean;
  createdAt: string;
  updatedAt?: string;
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
