import { useQuery } from '@tanstack/react-query';
import { fetchPositions } from '@api/account';

export const usePositions = (enabled = true) =>
  useQuery({
    queryKey: ['positions'],
    queryFn: fetchPositions,
    enabled,
    refetchInterval: 10000
  });
