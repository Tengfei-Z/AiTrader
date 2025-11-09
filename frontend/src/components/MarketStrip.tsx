import { Card, Skeleton, Typography } from 'antd';
import { useInitialEquity } from '@hooks/useInitialEquity';
import { useMultipleTickers } from '@hooks/useMultipleTickers';
import dayjs from 'dayjs';

type CoinConfig = {
  symbol: string;
  name: string;
  color: string;
  icon: string;
};

const coins: CoinConfig[] = [
  {
    symbol: 'BTC-USDT',
    name: 'Bitcoin',
    color: '#f97316',
    icon: '/icons/bitcoin.svg'
  },
  {
    symbol: 'ETH-USDT',
    name: 'Ethereum',
    color: '#3b82f6',
    icon: '/icons/ethereum.svg'
  },
  {
    symbol: 'SOL-USDT',
    name: 'Solana',
    color: '#14b8a6',
    icon: '/icons/solana.svg'
  },
  {
    symbol: 'BNB-USDT',
    name: 'BNB',
    color: '#fbbf24',
    icon: '/icons/bnb.svg'
  }
];

const formatNumber = (value?: number) =>
  value !== undefined ? value.toLocaleString(undefined, { maximumFractionDigits: 2 }) : '--';

const MarketStrip = () => {
  const { data, isLoading } = useMultipleTickers(coins.map((item) => item.symbol));
  const { data: initialEquityRecord } = useInitialEquity();
  const initialAmount = initialEquityRecord ? Number(initialEquityRecord.amount) : undefined;
  const initialRecordedAt = initialEquityRecord?.recordedAt;
  const initialAmountLabel = initialAmount !== undefined ? `${formatNumber(initialAmount)} USDT` : '--';

  if (isLoading) {
    return (
      <Card bordered={false}>
        <Skeleton active paragraph={{ rows: 1 }} />
      </Card>
    );
  }

  const initialChip = (
    <div className="market-strip-initial-chip">
      <Typography.Text className="market-strip-initial-label">初始金额</Typography.Text>
      <Typography.Title level={5} className="market-strip-initial-value">
        {initialAmountLabel}
      </Typography.Title>
      {initialRecordedAt && (
        <Typography.Text type="secondary" className="market-strip-initial-timestamp">
          记录于 {dayjs(initialRecordedAt).format('MM-DD HH:mm')}
        </Typography.Text>
      )}
    </div>
  );

  return (
    <Card bordered={false} className="market-strip">
      <div className="market-strip-body">
        <div className="market-strip-coin-row">
          {coins.map((coin) => {
            const ticker = data?.[coin.symbol];
            const price = ticker ? Number(ticker.last) : undefined;
            const high = ticker?.high24h ? Number(ticker.high24h) : undefined;
            const low = ticker?.low24h ? Number(ticker.low24h) : undefined;
            const mid = high && low ? (high + low) / 2 : undefined;
            const change = price && mid ? ((price - mid) / mid) * 100 : undefined;
            return (
              <div key={coin.symbol} className="market-strip-coin-block">
                <img
                  src={coin.icon}
                  alt={coin.name}
                  className="market-strip-coin-icon"
                  style={{ borderColor: coin.color }}
                />
                <Typography.Text className="market-strip-coin-name">{coin.name}</Typography.Text>
                <Typography.Title level={4} className="market-strip-coin-price">
                  {formatNumber(price)}
                </Typography.Title>
                <Typography.Text
                  className={`market-strip-coin-change ${
                    change === undefined ? '' : change >= 0 ? 'positive' : 'negative'
                  }`}
                >
                  {change !== undefined ? `${change >= 0 ? '+' : ''}${change.toFixed(2)}%` : '--'}
                </Typography.Text>
              </div>
            );
          })}
        </div>
        {initialChip}
      </div>
    </Card>
  );
};

export default MarketStrip;
