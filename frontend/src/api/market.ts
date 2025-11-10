import client from './client';
import type { ApiResponse, Ticker } from './types';

export const fetchTicker = async (symbol: string) => {
  const { data } = await client.get<ApiResponse<Ticker>>('/market/ticker', {
    params: { symbol }
  });
  return data.data;
};
