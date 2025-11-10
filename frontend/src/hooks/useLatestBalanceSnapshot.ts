import { useQuery } from '@tanstack/react-query';
import { fetchLatestBalanceSnapshot } from '@api/account';

export const useLatestBalanceSnapshot = (asset = 'USDT', enabled = true) =>
  useQuery({
    queryKey: ['balance-snapshot', 'latest', asset],
    queryFn: () => fetchLatestBalanceSnapshot(asset),
    enabled,
    refetchInterval: 10000
  });
