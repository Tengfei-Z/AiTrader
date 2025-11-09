import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { fetchInitialEquity, setInitialEquity } from '@api/account';
import type { InitialEquityRecord } from '@api/types';

export const useInitialEquity = () =>
  useQuery<InitialEquityRecord | null>({
    queryKey: ['initial-equity'],
    queryFn: fetchInitialEquity,
    staleTime: Infinity
  });

export const useSetInitialEquity = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (amount: number) => setInitialEquity({ amount }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['initial-equity'] })
  });
};
