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

export interface PositionItem {
  symbol: string;
  side: string;
  entry_price?: number;
  current_price?: number;
  quantity?: number;
  leverage?: number;
  liquidation_price?: number;
  margin?: number;
  unrealized_pnl?: number;
  entry_time?: string;
  take_profit_trigger?: number;
  take_profit_price?: number;
  take_profit_type?: string;
  stop_loss_trigger?: number;
  stop_loss_price?: number;
  stop_loss_type?: string;
}

export interface PositionHistoryItem {
  symbol: string;
  side: string;
  quantity?: number;
  leverage?: number;
  entry_price?: number;
  exit_price?: number;
  margin?: number;
  realized_pnl?: number;
  entry_time?: string;
  exit_time?: string;
}

export interface StrategyMessage {
  id: string;
  role: 'assistant' | 'user' | 'system';
  content: string;
  createdAt: string;
  summary?: string;
  tags?: string[];
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

export interface PlaceOrderPayload {
  symbol: string;
  side: 'buy' | 'sell';
  type: 'limit' | 'market';
  price?: string;
  size: string;
  timeInForce?: 'gtc' | 'ioc' | 'fok';
}

export interface PlaceOrderResponse {
  orderId: string;
  status: OrderStatus;
}
