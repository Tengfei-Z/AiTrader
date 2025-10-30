import { useQuery } from '@tanstack/react-query';
import { fetchOrderBook } from '@api/market';

export const useOrderBook = (symbol: string, depth = 50, enabled = true) =>
  useQuery({
    queryKey: ['orderbook', symbol, depth],
    queryFn: () => fetchOrderBook(symbol, depth),
    enabled,
    refetchInterval: 3000
  });
