import { useQuery } from '@tanstack/react-query';
import { fetchOpenOrders } from '@api/account';

export const useOpenOrders = (symbol?: string, enabled = true) =>
  useQuery({
    queryKey: ['open-orders', symbol],
    queryFn: () => fetchOpenOrders(symbol),
    enabled,
    refetchInterval: 8000
  });
