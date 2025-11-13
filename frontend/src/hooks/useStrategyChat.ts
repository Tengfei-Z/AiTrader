import { useQuery } from '@tanstack/react-query';
import { fetchStrategyChat } from '@api/model';

export const useStrategyChat = (enabled = true) =>
  useQuery({
    queryKey: ['strategy-chat'],
    queryFn: fetchStrategyChat,
    enabled,
    staleTime: 30_000,
    refetchInterval: 60_000,
    refetchIntervalInBackground: true
  });
