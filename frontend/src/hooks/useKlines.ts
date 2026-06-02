import { useQuery } from '@tanstack/react-query';
import { fetchKlines } from '@api/market';
import { DEFAULT_KLINE_REFETCH_INTERVAL } from './constants';

export const useKlines = (
  params: { symbol: string; bar?: string; limit?: number },
  enabled = true,
  live = true
) =>
  useQuery({
    queryKey: ['klines', params.symbol, params.bar, params.limit],
    queryFn: () => fetchKlines(params),
    enabled,
    refetchInterval: enabled && live ? DEFAULT_KLINE_REFETCH_INTERVAL : false
  });
