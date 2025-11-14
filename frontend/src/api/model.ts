import client from './client';
import type { ApiResponse, StrategyChatPayload } from './types';

export const fetchStrategyChat = async () => {
  const { data } = await client.get<ApiResponse<StrategyChatPayload>>('/model/strategy-chat');
  return (
    data.data ?? {
      allowManualTrigger: false,
      messages: []
    }
  );
};

export const triggerStrategyRun = async () => {
  await client.post<ApiResponse<null>>('/model/strategy-run');
};
