import client from './client';
import type { ApiResponse, OrderBook, Ticker, TradeItem } from './types';

export const fetchTicker = async (symbol: string) => {
  const { data } = await client.get<ApiResponse<Ticker>>('/market/ticker', {
    params: { symbol }
  });
  return data.data;
};

export const fetchOrderBook = async (symbol: string, depth = 50) => {
  const { data } = await client.get<ApiResponse<OrderBook>>('/market/orderbook', {
    params: { symbol, depth }
  });
  return data.data;
};

export const fetchRecentTrades = async (symbol: string, limit = 50) => {
  const { data } = await client.get<ApiResponse<TradeItem[]>>('/market/trades', {
    params: { symbol, limit }
  });
  return data.data;
};
