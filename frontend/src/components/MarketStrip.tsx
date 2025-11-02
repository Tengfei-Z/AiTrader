import { Avatar, Card, Flex, Skeleton, Tag, Typography } from 'antd';
import { useMultipleTickers } from '@hooks/useMultipleTickers';
import { useBalances } from '@hooks/useBalances';

type CoinConfig = {
  symbol: string;
  code: string;
  name: string;
  color: string;
};

const coins: CoinConfig[] = [
  { symbol: 'BTC-USDT', code: 'BTC', name: 'Bitcoin', color: '#f97316' },
  { symbol: 'ETH-USDT', code: 'ETH', name: 'Ethereum', color: '#6366f1' },
  { symbol: 'SOL-USDT', code: 'SOL', name: 'Solana', color: '#0ea5e9' },
  { symbol: 'BNB-USDT', code: 'BNB', name: 'BNB', color: '#facc15' }
];

const formatNumber = (value?: number) =>
  value !== undefined ? value.toLocaleString(undefined, { maximumFractionDigits: 2 }) : '--';

const MarketStrip = () => {
  const { data, isLoading } = useMultipleTickers(coins.map((item) => item.symbol));
  const { data: balances } = useBalances();

  const usdtBalance = balances?.find((item) => item.asset.toUpperCase() === 'USDT');
  const total = usdtBalance ? Number(usdtBalance.valuationUSDT ?? usdtBalance.available) : undefined;
  const available = usdtBalance ? Number(usdtBalance.available) : undefined;
  const locked = total !== undefined && available !== undefined ? Math.max(total - available, 0) : undefined;
  const isSimulated = import.meta.env.VITE_OKX_SIMULATED !== 'false';

  if (isLoading) {
    return (
      <Card bordered={false}>
        <Skeleton active paragraph={{ rows: 1 }} />
      </Card>
    );
  }

  return (
    <Card bordered={false} className="market-strip">
      <Flex className="market-strip-layout" wrap gap={24}>
        <div className="market-strip-ticker-wrapper">
          <Flex wrap gap={12} className="market-strip-ticker-grid">
            {coins.map((coin) => {
              const ticker = data?.[coin.symbol];
              const price = ticker ? Number(ticker.last) : undefined;
              const high = ticker?.high24h ? Number(ticker.high24h) : undefined;
              const low = ticker?.low24h ? Number(ticker.low24h) : undefined;
              const mid = high && low ? (high + low) / 2 : undefined;
              const change = price && mid ? ((price - mid) / mid) * 100 : undefined;
              const changeValue =
                change !== undefined ? `${change >= 0 ? '+' : ''}${change.toFixed(2)}%` : '--';

              return (
                <div key={coin.symbol} className="market-strip-ticker-card">
                  <Avatar size={36} style={{ backgroundColor: coin.color, color: '#fff' }}>
                    {coin.code.slice(0, 1)}
                  </Avatar>
                  <div className="market-strip-ticker-info">
                    <div className="market-strip-ticker-title">
                      <span className="market-strip-ticker-name">{coin.name}</span>
                      <span className="market-strip-ticker-code">{coin.code}</span>
                    </div>
                    <div className="market-strip-ticker-meta">
                      <span className="price">{formatNumber(price)}</span>
                      <span
                        className={`change ${change === undefined ? '' : change >= 0 ? 'positive' : 'negative'}`}
                      >
                        {changeValue}
                      </span>
                    </div>
                  </div>
                </div>
              );
            })}
          </Flex>
        </div>

        <div className="market-strip-balance-card">
          <div className="market-strip-balance-header">
            <div>
              <Typography.Text className="market-strip-balance-label">当前账户权益</Typography.Text>
              <div className="market-strip-balance-value">{formatNumber(total)}</div>
            </div>
            <Tag color={isSimulated ? 'geekblue' : 'green'}>{isSimulated ? 'SIMULATED' : 'LIVE'}</Tag>
          </div>
        </div>
      </Flex>
    </Card>
  );
};

export default MarketStrip;
