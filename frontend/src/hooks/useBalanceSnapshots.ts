import { useQuery, useQueryClient } from '@tanstack/react-query';
import { fetchAllBalanceSnapshots } from '@api/account';
import type { BalanceSnapshotItem } from '@api/types';

const DEFAULT_SNAPSHOT_LIMIT = 200;

export const useBalanceSnapshots = (
  params: { asset?: string; limit?: number } = {},
  enabled = true
) => {
  const asset = params.asset ?? 'USDT';
  const limit = params.limit ?? DEFAULT_SNAPSHOT_LIMIT;
  const queryClient = useQueryClient();
  const queryKey = ['balance-snapshots', asset, limit];

  return useQuery<BalanceSnapshotItem[]>({
    queryKey,
    queryFn: async () => {
      const existing = queryClient.getQueryData<BalanceSnapshotItem[]>(queryKey) ?? [];
      if (!existing.length) {
        return fetchAllBalanceSnapshots({ asset, limit });
      }

      const lastPoint = existing[existing.length - 1];
      if (!lastPoint) {
        return existing;
      }

      const appended = await fetchAllBalanceSnapshots({
        asset,
        limit,
        after: lastPoint.recordedAt
      });

      if (!appended.length) {
        return existing;
      }

      return [...existing, ...appended];
    },
    enabled,
    refetchInterval: 15000,
    gcTime: Infinity
  });
};
