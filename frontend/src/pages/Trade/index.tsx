import { Col, Flex, Row } from 'antd';
import { useSymbolStore } from '@store/useSymbolStore';
import OrderForm from '@components/OrderForm';
import OrderBookTable from '@components/OrderBookTable';
import RecentTradesTable from '@components/RecentTradesTable';
import TickerCard from '@components/TickerCard';
import { useOrderBook } from '@hooks/useOrderBook';
import { useRecentTrades } from '@hooks/useRecentTrades';
import { useTicker } from '@hooks/useTicker';
import OpenOrdersTable from '@components/OpenOrdersTable';
import { useOpenOrders } from '@hooks/useOpenOrders';

const TradePage = () => {
  const symbol = useSymbolStore((state) => state.symbol);
  const { data: ticker, isLoading: tickerLoading } = useTicker(symbol);
  const { data: orderBook, isLoading: orderBookLoading } = useOrderBook(symbol, 50);
  const { data: trades, isLoading: tradesLoading } = useRecentTrades(symbol, 80);
  const { data: openOrders, isLoading: openOrdersLoading } = useOpenOrders(symbol);

  return (
    <Flex vertical gap={24}>
      <TickerCard ticker={ticker} loading={tickerLoading} />
      <Row gutter={24}>
        <Col span={8}>
          <OrderForm symbol={symbol} />
        </Col>
        <Col span={8}>
          <OrderBookTable orderBook={orderBook} loading={orderBookLoading} />
        </Col>
        <Col span={8}>
          <RecentTradesTable trades={trades} loading={tradesLoading} />
        </Col>
      </Row>
      <OpenOrdersTable orders={openOrders} loading={openOrdersLoading} />
    </Flex>
  );
};

export default TradePage;
