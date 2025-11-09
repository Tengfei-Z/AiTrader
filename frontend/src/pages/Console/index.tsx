import { Col, Flex, Row, message } from 'antd';
import { useMemo } from 'react';
import EquityCurveCard from '@components/EquityCurveCard';
import MarketStrip from '@components/MarketStrip';
import PositionsHistoryCard from '@components/PositionsHistoryCard';
import { useBalances } from '@hooks/useBalances';
import { useFills } from '@hooks/useFills';
import { useInitialEquity } from '@hooks/useInitialEquity';
import { usePositionHistory } from '@hooks/usePositionHistory';
import { useStrategyChat } from '@hooks/useStrategyChat';
import { useStrategyRunner } from '@hooks/useStrategyRunner';
import { usePositions } from '@hooks/usePositions';
import { useTicker } from '@hooks/useTicker';
import { useSymbolStore } from '@store/useSymbolStore';
import { buildEquityCurve } from '@utils/pnl';

const safelyParseNumber = (value?: string) => {
  if (value === undefined) {
    return undefined;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
};

const AiConsolePage = () => {
  const symbol = useSymbolStore((state) => state.symbol);

  const { data: ticker } = useTicker(symbol);
  const { data: balances } = useBalances();
  const { data: fills, isLoading: fillsLoading } = useFills(symbol, 200);
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

  const markPrice = ticker?.last ? Number(ticker.last) : undefined;
  const initialAmount = initialEquityRecord ? Number(initialEquityRecord.amount) : undefined;

  const equityStats = useMemo(
    () => buildEquityCurve(fills, markPrice, initialAmount),
    [fills, markPrice, initialAmount]
  );
  const equityCurve = equityStats.points;
  const currentEquity = equityCurve.length
    ? equityCurve[equityCurve.length - 1]?.equity
    : initialAmount;
  const currentAccountValue = useMemo(() => {
    if (!balances?.length) {
      return undefined;
    }
    let hasValue = false;
    const total = balances.reduce((sum, balance) => {
      const valuation = safelyParseNumber(balance.valuationUSDT);
      if (valuation !== undefined) {
        hasValue = true;
        return sum + valuation;
      }
      const available = safelyParseNumber(balance.available);
      const locked = safelyParseNumber(balance.locked);
      if (available !== undefined || locked !== undefined) {
        hasValue = true;
      }
      return sum + (available ?? 0) + (locked ?? 0);
    }, 0);
    return hasValue ? total : undefined;
  }, [balances]);
  const resolvedCurrentEquity = currentAccountValue ?? currentEquity;
  const profitPercent =
    initialAmount && resolvedCurrentEquity !== undefined
      ? ((resolvedCurrentEquity - initialAmount) / initialAmount) * 100
      : undefined;

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
            loading={fillsLoading}
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
