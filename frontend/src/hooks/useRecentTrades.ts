import { useQuery } from '@tanstack/react-query';
import { fetchRecentTrades } from '@api/market';

export const useRecentTrades = (symbol: string, limit = 50, enabled = true) =>
  useQuery({
    queryKey: ['recent-trades', symbol, limit],
    queryFn: () => fetchRecentTrades(symbol, limit),
    enabled,
    refetchInterval: 4000
  });
