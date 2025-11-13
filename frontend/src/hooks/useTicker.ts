import { useQuery } from '@tanstack/react-query';
import { fetchTicker } from '@api/market';
import { DEFAULT_TICKER_REFETCH_INTERVAL } from './constants';

export const useTicker = (symbol: string, enabled = true) =>
  useQuery({
    queryKey: ['ticker', symbol],
    queryFn: () => fetchTicker(symbol),
    enabled,
    refetchInterval: DEFAULT_TICKER_REFETCH_INTERVAL
  });
