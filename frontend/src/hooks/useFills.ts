import { useQuery } from '@tanstack/react-query';
import { fetchFills } from '@api/account';

export const useFills = (symbol?: string, limit = 50, enabled = true) =>
  useQuery({
    queryKey: ['fills', symbol, limit],
    queryFn: () => fetchFills({ symbol, limit }),
    enabled,
    refetchInterval: 10000
  });
