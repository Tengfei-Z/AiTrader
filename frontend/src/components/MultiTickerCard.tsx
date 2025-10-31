import { Card, Col, Row, Statistic, Typography } from 'antd';
import dayjs from 'dayjs';
import { useMultipleTickers } from '@hooks/useMultipleTickers';

const symbols = ['BTC-USDT', 'ETH-USDT', 'BNB-USDT', 'SOL-USDT'];

const MultiTickerCard = () => {
  const { data, isLoading } = useMultipleTickers(symbols);

  return (
    <Card bordered={false} loading={isLoading}>
      <Row gutter={[16, 16]}>
        {symbols.map((symbol) => {
          const ticker = data?.[symbol];
          const price = ticker ? Number(ticker.last) : undefined;
          const low = ticker?.low24h ? Number(ticker.low24h) : undefined;
          const change = ticker && low && low !== 0 ? ((Number(ticker.last) - low) / low) * 100 : undefined;
          const bid = ticker?.bidPx ? Number(ticker.bidPx) : undefined;
          return (
            <Col key={symbol} xs={24} sm={12} md={6}>
              <div className="multi-ticker-item">
                <Typography.Text type="secondary">{symbol}</Typography.Text>
                <Typography.Title level={3} style={{ margin: '8px 0' }}>
                  {price?.toLocaleString(undefined, { maximumFractionDigits: 2 }) ?? '--'}
                </Typography.Title>
                <Statistic
                  title="24h 涨跌"
                  value={change}
                  precision={2}
                  suffix="%"
                  valueStyle={{ color: price && bid ? (price >= bid ? '#16a34a' : '#dc2626') : undefined }}
                />
                <Typography.Text type="secondary">
                  更新时间：
                  {ticker ? dayjs(Number(ticker.timestamp)).format('HH:mm:ss') : '--'}
                </Typography.Text>
              </div>
            </Col>
          );
        })}
      </Row>
    </Card>
  );
};

export default MultiTickerCard;
