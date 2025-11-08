import { useMutation, useQueryClient } from '@tanstack/react-query';
import { triggerStrategyRun } from '@api/model';

export const useStrategyRunner = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: triggerStrategyRun,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['strategy-chat'] });
    }
  });
};
