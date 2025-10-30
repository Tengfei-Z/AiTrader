import client from './client';
import type { ApiResponse, BalanceItem, FillItem, OrderItem } from './types';

export const fetchBalances = async () => {
  const { data } = await client.get<ApiResponse<BalanceItem[]>>('/account/balances');
  return data.data;
};

export const fetchOpenOrders = async (symbol?: string) => {
  const { data } = await client.get<ApiResponse<OrderItem[]>>('/account/orders/open', {
    params: { symbol }
  });
  return data.data;
};

export const fetchOrderHistory = async (params: { symbol?: string; limit?: number; state?: string }) => {
  const { data } = await client.get<ApiResponse<OrderItem[]>>('/account/orders/history', {
    params
  });
  return data.data;
};

export const fetchFills = async (params: { symbol?: string; limit?: number }) => {
  const { data } = await client.get<ApiResponse<FillItem[]>>('/account/fills', {
    params
  });
  return data.data;
};
