import client from './client';
import type { ApiResponse, KlinePayload, Ticker } from './types';

export const fetchTicker = async (symbol: string) => {
  const { data } = await client.get<ApiResponse<Ticker>>('/market/ticker', {
    params: { symbol }
  });
  return data.data;
};

export const fetchKlines = async (params: { symbol: string; bar?: string; limit?: number }) => {
  const { data } = await client.get<ApiResponse<KlinePayload>>('/market/klines', {
    params
  });
  return data.data;
};
