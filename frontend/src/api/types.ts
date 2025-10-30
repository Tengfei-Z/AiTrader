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
