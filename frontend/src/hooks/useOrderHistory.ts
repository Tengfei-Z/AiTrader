import { useQuery } from '@tanstack/react-query';
import { fetchOrderHistory } from '@api/account';

export const useOrderHistory = (params: { symbol?: string; limit?: number; state?: string }, enabled = true) =>
  useQuery({
    queryKey: ['order-history', params],
    queryFn: () => fetchOrderHistory(params),
    enabled,
    refetchInterval: 15000
  });
