import { useQuery } from '@tanstack/react-query';
import { fetchPositionHistory } from '@api/account';

export const usePositionHistory = (enabled = true) =>
  useQuery({
    queryKey: ['positions-history'],
    queryFn: () => fetchPositionHistory(),
    enabled,
    refetchInterval: 30000
  });
