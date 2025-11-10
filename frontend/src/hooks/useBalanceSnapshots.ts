import { useQuery } from '@tanstack/react-query';
import { fetchBalanceSnapshots } from '@api/account';

const DEFAULT_SNAPSHOT_LIMIT = 200;

export const useBalanceSnapshots = (
  params: { asset?: string; limit?: number } = {},
  enabled = true
) => {
  const asset = params.asset ?? 'USDT';
  const limit = params.limit ?? DEFAULT_SNAPSHOT_LIMIT;

  return useQuery({
    queryKey: ['balance-snapshots', asset, limit],
    queryFn: () => fetchBalanceSnapshots({ asset, limit }),
    enabled,
    refetchInterval: 15000
  });
};
