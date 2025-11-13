import { useQueries } from '@tanstack/react-query';
import { fetchTicker } from '@api/market';
import type { Ticker } from '@api/types';
import { DEFAULT_TICKER_REFETCH_INTERVAL } from './constants';

export const useMultipleTickers = (symbols: string[]) => {
  const results = useQueries({
    queries: symbols.map((symbol) => ({
      queryKey: ['ticker', symbol],
      queryFn: () => fetchTicker(symbol),
      refetchInterval: DEFAULT_TICKER_REFETCH_INTERVAL,
    })),
  });

  const isLoading = results.some((result) => result.isLoading);
  const data = results.reduce<Record<string, Ticker | undefined>>((acc, result, index) => {
    acc[symbols[index]] = result.data;
    return acc;
  }, {});

  return { data, isLoading };
};
