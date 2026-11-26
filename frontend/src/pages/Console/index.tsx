import { Col, Flex, Row, message } from 'antd';
import { useMutation } from '@tanstack/react-query';
import { useMemo } from 'react';
import EquityCurveCard from '@components/EquityCurveCard';
import MarketStrip from '@components/MarketStrip';
import PositionsHistoryCard from '@components/PositionsHistoryCard';
import RealtimeKlineChart from '@components/RealtimeKlineChart';
import { useBalanceSnapshots } from '@hooks/useBalanceSnapshots';
import { useLatestBalanceSnapshot } from '@hooks/useLatestBalanceSnapshot';
import { useInitialEquity } from '@hooks/useInitialEquity';
import { usePositionHistory } from '@hooks/usePositionHistory';
import { useStrategyChat } from '@hooks/useStrategyChat';
import { useStrategyRunner } from '@hooks/useStrategyRunner';
import { usePositions } from '@hooks/usePositions';
import { updateModelBudgetLimit } from '@api/model';
import type { BalanceSnapshotItem } from '@api/types';
import type { EquityPoint } from '@utils/pnl';

const AiConsolePage = () => {
  const { data: balanceSnapshots, isLoading: snapshotsLoading } = useBalanceSnapshots({ limit: 200 });
  const { data: latestSnapshot } = useLatestBalanceSnapshot();
  const { data: positions, isLoading: positionsLoading } = usePositions();
  const { data: positionHistory, isLoading: positionHistoryLoading } = usePositionHistory();
  const {
    data: strategyChat,
    isLoading: strategyLoading,
    refetch: refetchStrategyChat
  } = useStrategyChat();
  const { data: initialEquityRecord } = useInitialEquity();
  const {
    mutateAsync: triggerStrategy,
    isPending: strategyRunning
  } = useStrategyRunner();
  const {
    mutateAsync: updateBudgetLimit,
    isPending: budgetLimitUpdating
  } = useMutation({
    mutationFn: updateModelBudgetLimit
  });

  const initialAmount = initialEquityRecord ? Number(initialEquityRecord.amount) : undefined;

  const snapshotCurve = useMemo(() => {
    if (!balanceSnapshots?.length) {
      return [];
    }
    return (
      balanceSnapshots
        .map((snapshot: BalanceSnapshotItem): EquityPoint | null => {
          const recordedAt = new Date(snapshot.recordedAt).getTime();
          const valuation = Number(snapshot.valuation);
          if (!Number.isFinite(recordedAt) || !Number.isFinite(valuation)) {
            return null;
          }
          return { time: recordedAt, equity: valuation };
        })
        .filter((point): point is EquityPoint => point !== null)
        .sort((a, b) => a.time - b.time)
    );
  }, [balanceSnapshots]);

  const initialEquityPoint = useMemo(() => {
    if (!initialEquityRecord) {
      return null;
    }
    const recordedAt = new Date(initialEquityRecord.recordedAt).getTime();
    const amount = Number(initialEquityRecord.amount);
    if (!Number.isFinite(recordedAt) || !Number.isFinite(amount)) {
      return null;
    }
    return { time: recordedAt, equity: amount };
  }, [initialEquityRecord]);

  const equityCurve = useMemo(() => {
    if (!initialEquityPoint) {
      return snapshotCurve;
    }

    const merged = [...snapshotCurve];
    const existingIndex = merged.findIndex(
      (point) => Math.abs(point.time - initialEquityPoint.time) < 1000
    );

    if (existingIndex >= 0) {
      merged[existingIndex] = { ...merged[existingIndex], equity: initialEquityPoint.equity };
    } else {
      merged.unshift(initialEquityPoint);
    }

    return merged.sort((a, b) => a.time - b.time);
  }, [initialEquityPoint, snapshotCurve]);

  const currentEquity = equityCurve.length
    ? equityCurve[equityCurve.length - 1]?.equity
    : initialAmount;
  const currentAccountValue = useMemo(() => {
    if (!latestSnapshot?.valuation) {
      return undefined;
    }
    const parsed = Number(latestSnapshot.valuation);
    return Number.isFinite(parsed) ? parsed : undefined;
  }, [latestSnapshot]);
  const resolvedCurrentEquity = currentAccountValue ?? currentEquity ?? initialAmount;
  const profitPercent =
    initialAmount && resolvedCurrentEquity !== undefined
      ? ((resolvedCurrentEquity - initialAmount) / initialAmount) * 100
      : undefined;
  const dailyProfitPercent = useMemo(() => {
    if (resolvedCurrentEquity === undefined || equityCurve.length === 0) {
      return undefined;
    }

    const todayStart = new Date();
    todayStart.setHours(0, 0, 0, 0);
    const baseline = equityCurve.find((point) => point.time >= todayStart.getTime())?.equity;
    if (baseline === undefined || baseline <= 0) {
      return undefined;
    }

    return ((resolvedCurrentEquity - baseline) / baseline) * 100;
  }, [equityCurve, resolvedCurrentEquity]);
  const chartLoading = snapshotsLoading && !snapshotCurve.length;
  const activeSymbol = useMemo(() => {
    const activePosition = positions?.find((position) => Math.abs(position.size) > 0);
    return activePosition?.instId ?? positions?.[0]?.instId ?? 'BTC-USDT-SWAP';
  }, [positions]);

  const handleStrategyStart = async () => {
    const startedAt = new Date().toISOString();
    console.info('[Strategy] Start button clicked, requesting /model/strategy-run', { startedAt });
    try {
      await triggerStrategy();
      console.info('[Strategy] Strategy run dispatched', {
        startedAt,
        finishedAt: new Date().toISOString()
      });
      message.success('策略运行已提交');
    } catch (error) {
      console.error('[Strategy] Strategy run failed', error);
      message.error('策略运行失败，请稍后重试');
    } finally {
      console.info('[Strategy] Refreshing chat history after trigger');
      await refetchStrategyChat();
    }
  };

  const handleBudgetLimitChange = async (limit: number) => {
    try {
      await updateBudgetLimit(limit);
      message.success('最大 AI 请求次数已更新');
      await refetchStrategyChat();
    } catch (error) {
      console.error('[Strategy] Failed to update model budget limit', error);
      message.error(error instanceof Error ? error.message : '更新最大 AI 请求次数失败');
      throw error;
    }
  };

  return (
    <Flex vertical gap={24}>
      <MarketStrip
        initialAmount={initialAmount}
        currentAmount={resolvedCurrentEquity}
        profitPercent={profitPercent}
        dailyProfitPercent={dailyProfitPercent}
      />

      <Row gutter={[24, 24]} align="stretch">
        <Col xs={24} xl={13} style={{ display: 'flex' }}>
          <EquityCurveCard
            data={equityCurve}
            loading={chartLoading}
            className="full-height-card"
            klineContent={
              <RealtimeKlineChart
                strategyMessages={strategyChat?.messages}
                positions={positions}
                defaultSymbol={activeSymbol}
              />
            }
          />
        </Col>
        <Col xs={24} xl={11} style={{ display: 'flex' }}>
          <PositionsHistoryCard
            positions={positions}
            positionsLoading={positionsLoading}
            history={positionHistory}
            historyLoading={positionHistoryLoading}
            strategyMessages={strategyChat?.messages}
            modelBudget={strategyChat?.modelBudget}
            strategyLoading={strategyLoading}
            manualTriggerEnabled={strategyChat?.allowManualTrigger}
            onStrategyStart={handleStrategyStart}
            onBudgetLimitChange={handleBudgetLimitChange}
            strategyRunning={strategyRunning}
            budgetLimitUpdating={budgetLimitUpdating}
            className="full-height-card"
          />
        </Col>
      </Row>
    </Flex>
  );
};

export default AiConsolePage;
