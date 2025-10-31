import { Col, Flex, Row } from 'antd';
import { useMemo } from 'react';
import AutomationStatusCard from '@components/AutomationStatusCard';
import EquityCurveCard from '@components/EquityCurveCard';
import MarketStrip from '@components/MarketStrip';
import OpenOrdersTable from '@components/OpenOrdersTable';
import OrderHistoryTable from '@components/OrderHistoryTable';
import PositionCard from '@components/PositionCard';
import TradeActivityList from '@components/TradeActivityList';
import { useFills } from '@hooks/useFills';
import { useOpenOrders } from '@hooks/useOpenOrders';
import { useOrderHistory } from '@hooks/useOrderHistory';
import { useTicker } from '@hooks/useTicker';
import { useSymbolStore } from '@store/useSymbolStore';
import { buildEquityCurve } from '@utils/pnl';

const AiConsolePage = () => {
  const symbol = useSymbolStore((state) => state.symbol);

  const { data: ticker } = useTicker(symbol);
  const { data: openOrders, isLoading: openOrdersLoading } = useOpenOrders(symbol);
  const { data: fills, isLoading: fillsLoading } = useFills(symbol, 200);
  const { data: orderHistory, isLoading: historyLoading } = useOrderHistory({ symbol, limit: 120 }, true);

  const markPrice = ticker?.last ? Number(ticker.last) : undefined;

  const { points: equityCurve, netPosition, avgEntryPrice, unrealizedPnl } = useMemo(
    () => buildEquityCurve(fills, markPrice),
    [fills, markPrice]
  );

  return (
    <Flex vertical gap={24}>
      <MarketStrip />

      <Row gutter={[24, 24]}>
        <Col xs={24} xl={16}>
          <Flex vertical gap={24}>
            <EquityCurveCard data={equityCurve} loading={fillsLoading} />
            <PositionCard
              symbol={symbol}
              netPosition={netPosition}
              avgEntryPrice={avgEntryPrice}
              markPrice={markPrice}
              unrealizedPnl={unrealizedPnl}
            />
          </Flex>
        </Col>
        <Col xs={24} xl={8}>
          <TradeActivityList fills={fills} loading={fillsLoading} />
          <AutomationStatusCard
            symbol={symbol}
            fills={fills}
            openOrders={openOrders}
            loading={fillsLoading || openOrdersLoading}
          />
        </Col>
      </Row>

      <Row gutter={[24, 24]}>
        <Col xs={24} md={12}>
          <OrderHistoryTable orders={orderHistory} loading={historyLoading} />
        </Col>
        <Col xs={24} md={12}>
          <OpenOrdersTable orders={openOrders} loading={openOrdersLoading} />
        </Col>
      </Row>
    </Flex>
  );
};

export default AiConsolePage;
