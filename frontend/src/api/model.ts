import client from './client';
import type { ApiResponse, ModelBudget, StrategyChatPayload } from './types';

export const fetchStrategyChat = async () => {
  const { data } = await client.get<ApiResponse<StrategyChatPayload>>('/model/strategy-chat');
  return (
    data.data ?? {
      allowManualTrigger: false,
      modelBudget: {
        remaining: 0,
        limit: 0,
        used: 0
      },
      messages: []
    }
  );
};

export const triggerStrategyRun = async () => {
  await client.post<ApiResponse<null>>('/model/strategy-run');
};

export const updateModelBudgetLimit = async (limit: number) => {
  const { data } = await client.post<ApiResponse<ModelBudget>>('/model/strategy-budget', { limit });
  if (!data.success) {
    throw new Error(data.error ?? '更新大模型请求上限失败');
  }
  return data.data;
};
