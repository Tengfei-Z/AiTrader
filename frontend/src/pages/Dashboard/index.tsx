import { Col, Flex, Row, Spin } from 'antd';
import { useMemo } from 'react';
import { useSymbolStore } from '@store/useSymbolStore';
import { useOrderBook } from '@hooks/useOrderBook';
import { useRecentTrades } from '@hooks/useRecentTrades';
import { useTicker } from '@hooks/useTicker';
import PriceChart from '@components/PriceChart';
import OrderBookTable from '@components/OrderBookTable';
import RecentTradesTable from '@components/RecentTradesTable';
import TickerCard from '@components/TickerCard';

const DashboardPage = () => {
  const symbol = useSymbolStore((state) => state.symbol);
  const { data: ticker, isLoading: tickerLoading } = useTicker(symbol);
  const { data: orderBook, isLoading: orderBookLoading } = useOrderBook(symbol, 50);
  const { data: trades, isLoading: tradesLoading } = useRecentTrades(symbol, 80);

  const chartData = useMemo(() => {
    if (!trades) return [];
    return trades
      .slice()
      .reverse()
      .map((trade) => ({ time: trade.timestamp, price: Number(trade.price) }));
  }, [trades]);

  const loading = tickerLoading && orderBookLoading && tradesLoading;

  return (
    <Flex vertical gap={24}>
      <TickerCard ticker={ticker} loading={tickerLoading} />
      {loading ? (
        <Spin />
      ) : (
        <Row gutter={24}>
          <Col span={16}>
            <PriceChart data={chartData} loading={tradesLoading} />
          </Col>
          <Col span={8}>
            <OrderBookTable orderBook={orderBook} loading={orderBookLoading} />
          </Col>
        </Row>
      )}
      <RecentTradesTable trades={trades} loading={tradesLoading} />
    </Flex>
  );
};

export default DashboardPage;
