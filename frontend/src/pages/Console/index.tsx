import { Col, Flex, Row, message } from 'antd';
import { useMemo } from 'react';
import EquityCurveCard from '@components/EquityCurveCard';
import MarketStrip from '@components/MarketStrip';
import PositionsHistoryCard from '@components/PositionsHistoryCard';
import { useFills } from '@hooks/useFills';
import { usePositionHistory } from '@hooks/usePositionHistory';
import { useStrategyChat } from '@hooks/useStrategyChat';
import { useStrategyRunner } from '@hooks/useStrategyRunner';
import { usePositions } from '@hooks/usePositions';
import { useTicker } from '@hooks/useTicker';
import { useSymbolStore } from '@store/useSymbolStore';
import { buildEquityCurve } from '@utils/pnl';

const AiConsolePage = () => {
  const symbol = useSymbolStore((state) => state.symbol);

  const { data: ticker } = useTicker(symbol);
  const { data: fills, isLoading: fillsLoading } = useFills(symbol, 200);
  const { data: positions, isLoading: positionsLoading } = usePositions();
  const { data: positionHistory, isLoading: positionHistoryLoading } = usePositionHistory();
  const {
    data: strategyMessages,
    isLoading: strategyLoading,
    refetch: refetchStrategyChat
  } = useStrategyChat();
  const {
    mutateAsync: triggerStrategy,
    isPending: strategyRunning
  } = useStrategyRunner();

  const markPrice = ticker?.last ? Number(ticker.last) : undefined;

  const { points: equityCurve } = useMemo(
    () => buildEquityCurve(fills, markPrice),
    [fills, markPrice]
  );

  const handleStrategyStart = async () => {
    const startedAt = new Date().toISOString();
    console.info('[Strategy] Start button clicked, requesting /model/strategy-run', { startedAt });
    try {
      const messages = await triggerStrategy();
      console.info('[Strategy] Strategy run finished', {
        startedAt,
        finishedAt: new Date().toISOString(),
        messageCount: messages?.length ?? 0
      });
      message.success('已触发策略运行');
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
      <MarketStrip />

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
