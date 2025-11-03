import { Col, Flex, Row } from 'antd';
import { useMemo } from 'react';
import EquityCurveCard from '@components/EquityCurveCard';
import MarketStrip from '@components/MarketStrip';
import PositionsHistoryCard from '@components/PositionsHistoryCard';
import { useFills } from '@hooks/useFills';
import { usePositionHistory } from '@hooks/usePositionHistory';
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

  const markPrice = ticker?.last ? Number(ticker.last) : undefined;

  const { points: equityCurve } = useMemo(
    () => buildEquityCurve(fills, markPrice),
    [fills, markPrice]
  );

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
            className="full-height-card"
          />
        </Col>
      </Row>
    </Flex>
  );
};

export default AiConsolePage;
