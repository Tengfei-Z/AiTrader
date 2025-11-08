import client from './client';
import type { ApiResponse, StrategyMessage } from './types';

export const fetchStrategyChat = async () => {
  const { data } = await client.get<ApiResponse<StrategyMessage[]>>('/model/strategy-chat');
  return data.data ?? [];
};

export const triggerStrategyRun = async () => {
  await client.post<ApiResponse<null>>('/model/strategy-run');
};
