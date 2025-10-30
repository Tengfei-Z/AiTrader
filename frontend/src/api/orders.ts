import client from './client';
import type { ApiResponse, PlaceOrderPayload, PlaceOrderResponse } from './types';

export const placeOrder = async (payload: PlaceOrderPayload) => {
  const { data } = await client.post<ApiResponse<PlaceOrderResponse>>('/orders', payload);
  return data.data;
};

export const cancelOrder = async (orderId: string) => {
  const { data } = await client.delete<ApiResponse<{ orderId: string; status: string }>>(`/orders/${orderId}`);
  return data.data;
};
