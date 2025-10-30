import { useQuery } from '@tanstack/react-query';
import { fetchBalances } from '@api/account';

export const useBalances = (enabled = true) =>
  useQuery({
    queryKey: ['balances'],
    queryFn: fetchBalances,
    enabled,
    refetchInterval: 10000
  });
