import { Col, Flex, Row, message } from 'antd';
import { useMemo } from 'react';
import EquityCurveCard from '@components/EquityCurveCard';
import MarketStrip from '@components/MarketStrip';
import PositionsHistoryCard from '@components/PositionsHistoryCard';
import { useBalanceSnapshots } from '@hooks/useBalanceSnapshots';
import { useLatestBalanceSnapshot } from '@hooks/useLatestBalanceSnapshot';
import { useInitialEquity } from '@hooks/useInitialEquity';
import { usePositionHistory } from '@hooks/usePositionHistory';
import { useStrategyChat } from '@hooks/useStrategyChat';
import { useStrategyRunner } from '@hooks/useStrategyRunner';
import { usePositions } from '@hooks/usePositions';
import type { BalanceSnapshotItem } from '@api/types';
import type { EquityPoint } from '@utils/pnl';

const AiConsolePage = () => {
  const { data: balanceSnapshots, isLoading: snapshotsLoading } = useBalanceSnapshots({ limit: 200 });
  const { data: latestSnapshot } = useLatestBalanceSnapshot();
  const { data: positions, isLoading: positionsLoading } = usePositions();
  const { data: positionHistory, isLoading: positionHistoryLoading } = usePositionHistory();
  const {
    data: strategyMessages,
    isLoading: strategyLoading,
    refetch: refetchStrategyChat
  } = useStrategyChat();
  const { data: initialEquityRecord } = useInitialEquity();
  const {
    mutateAsync: triggerStrategy,
    isPending: strategyRunning
  } = useStrategyRunner();

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
  const chartLoading = snapshotsLoading && !snapshotCurve.length;

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

  return (
    <Flex vertical gap={24}>
      <MarketStrip
        initialAmount={initialAmount}
        currentAmount={resolvedCurrentEquity}
        profitPercent={profitPercent}
      />

      <Row gutter={[24, 24]} align="stretch">
        <Col xs={24} xl={14} style={{ display: 'flex' }}>
          <EquityCurveCard
            data={equityCurve}
            loading={chartLoading}
            className="full-height-card"
          />
        </Col>
        <Col xs={24} xl={10} style={{ display: 'flex' }}>
          <PositionsHistoryCard
            positions={positions}
            positionsLoading={positionsLoading}
            history={positionHistory}
            historyLoading={positionHistoryLoading}
            strategyMessages={strategyMessages}
            strategyLoading={strategyLoading}
            onStrategyRefresh={refetchStrategyChat}
            onStrategyStart={handleStrategyStart}
            strategyRunning={strategyRunning}
            className="full-height-card"
          />
        </Col>
      </Row>
    </Flex>
  );
};

export default AiConsolePage;
