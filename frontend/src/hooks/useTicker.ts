import { useQuery } from '@tanstack/react-query';
import { fetchTicker } from '@api/market';

export const useTicker = (symbol: string, enabled = true) =>
  useQuery({
    queryKey: ['ticker', symbol],
    queryFn: () => fetchTicker(symbol),
    enabled,
    refetchInterval: 5000
  });
