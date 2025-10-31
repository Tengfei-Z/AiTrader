import { Card, Flex, Skeleton, Statistic, Typography } from 'antd';
import { useMultipleTickers } from '@hooks/useMultipleTickers';

const symbols = ['BTC-USDT', 'ETH-USDT', 'SOL-USDT', 'BNB-USDT', 'DOGE-USDT'];

const formatPrice = (value?: number) =>
  value !== undefined ? value.toLocaleString(undefined, { maximumFractionDigits: 2 }) : '--';

const MarketStrip = () => {
  const { data, isLoading } = useMultipleTickers(symbols);

  if (isLoading) {
    return (
      <Card bordered={false}>
        <Skeleton active paragraph={{ rows: 1 }} />
      </Card>
    );
  }

  return (
    <Card bordered={false} className="market-strip">
      <Flex wrap gap={24} align="center">
        {symbols.map((symbol) => {
          const ticker = data?.[symbol];
          const price = ticker ? Number(ticker.last) : undefined;
          const high = ticker?.high24h ? Number(ticker.high24h) : undefined;
          const low = ticker?.low24h ? Number(ticker.low24h) : undefined;
          const mid = high && low ? (high + low) / 2 : undefined;
          const change = price && mid ? ((price - mid) / mid) * 100 : undefined;
          const changeValue = change !== undefined ? Number(change.toFixed(2)) : '--';
          const changeColor =
            change !== undefined ? (change >= 0 ? '#16a34a' : '#dc2626') : '#6b7280';

          return (
            <Flex key={symbol} align="center" gap={12}>
              <div className="market-strip-symbol">
                <Typography.Text strong>{symbol.replace('-USDT', '')}</Typography.Text>
              </div>
              <Flex vertical gap={4}>
                <Typography.Title level={5} style={{ margin: 0 }}>
                  {formatPrice(price)}
                </Typography.Title>
                <Statistic
                  title="24h"
                  value={changeValue}
                  precision={change !== undefined ? 2 : undefined}
                  suffix={change !== undefined ? '%' : undefined}
                  valueStyle={{ color: changeColor }}
                />
              </Flex>
            </Flex>
          );
        })}
      </Flex>
    </Card>
  );
};

export default MarketStrip;
