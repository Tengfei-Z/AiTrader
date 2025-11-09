import { useMutation } from '@tanstack/react-query';
import { triggerStrategyRun } from '@api/model';

export const useStrategyRunner = () =>
  useMutation({
    mutationFn: triggerStrategyRun
  });
