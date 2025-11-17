import { Card, Skeleton, Typography, Grid } from 'antd';
import { useMultipleTickers } from '@hooks/useMultipleTickers';

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
  },
  {
    symbol: 'OKB-USDT',
    name: 'OKX',
    color: '#0f172a',
    icon: '/icons/okx.svg'
  }
];

const formatNumber = (value?: number) =>
  value !== undefined ? value.toLocaleString(undefined, { maximumFractionDigits: 2 }) : '--';

interface MarketStripProps {
  initialAmount?: number;
  currentAmount?: number;
  profitPercent?: number;
}

const MarketStrip = ({
  initialAmount,
  currentAmount,
  profitPercent
}: MarketStripProps) => {
  const screens = Grid.useBreakpoint();
  const isMobile = !screens.md;
  const { data, isLoading } = useMultipleTickers(coins.map((item) => item.symbol));
  const initialAmountLabel = initialAmount !== undefined ? formatNumber(initialAmount) : '--';
  const resolvedCurrentAmount = currentAmount ?? initialAmount;
  const currentAmountLabel =
    resolvedCurrentAmount !== undefined ? formatNumber(resolvedCurrentAmount) : '--';
  const profitPercentLabel =
    profitPercent !== undefined ? `${profitPercent >= 0 ? '+' : ''}${profitPercent.toFixed(2)}%` : '--';
  const profitClass =
    profitPercent === undefined ? '' : profitPercent >= 0 ? 'positive' : 'negative';

  if (isLoading) {
    return (
      <Card bordered={false}>
        <Skeleton active paragraph={{ rows: 1 }} />
      </Card>
    );
  }

  const initialChip = (
    <div className={`market-strip-initial-chip ${isMobile ? 'is-mobile' : ''}`}>
      {isMobile ? (
        <div className="market-strip-metrics">
          <div className="market-strip-metrics-item">
            <Typography.Text className="market-strip-summary-label">初始金额</Typography.Text>
            <Typography.Text className="market-strip-summary-value market-strip-number">
              {initialAmountLabel}
            </Typography.Text>
          </div>
          <div className="market-strip-metrics-item">
            <Typography.Text className="market-strip-summary-label">当前金额</Typography.Text>
            <Typography.Text className="market-strip-summary-value market-strip-number">
              {currentAmountLabel}
            </Typography.Text>
          </div>
          {profitPercent !== undefined && (
            <div className="market-strip-metrics-item">
              <Typography.Text className="market-strip-summary-label">涨跌幅</Typography.Text>
              <Typography.Text
                className={`market-strip-summary-percent market-strip-number ${profitClass}`}
              >
                {profitPercentLabel}
              </Typography.Text>
            </div>
          )}
        </div>
      ) : (
        <div className="market-strip-summary">
          <div className="market-strip-summary-block">
            <Typography.Text className="market-strip-summary-label">初始金额</Typography.Text>
            <Typography.Text className="market-strip-summary-value market-strip-number">
              {initialAmountLabel}
            </Typography.Text>
          </div>
          <span className="market-strip-summary-divider" aria-hidden />
          <div className="market-strip-summary-block">
            <Typography.Text className="market-strip-summary-label">当前金额</Typography.Text>
            <div className="market-strip-summary-current">
              <Typography.Text className="market-strip-summary-value market-strip-number">
                {currentAmountLabel}
              </Typography.Text>
            </div>
          </div>
          {profitPercent !== undefined && (
            <>
              <span className="market-strip-summary-divider" aria-hidden />
              <div className="market-strip-summary-block market-strip-summary-block--narrow">
                <Typography.Text className="market-strip-summary-label">涨跌幅</Typography.Text>
                <Typography.Text
                  className={`market-strip-summary-percent market-strip-number ${profitClass}`}
                >
                  {profitPercentLabel}
                </Typography.Text>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );

  return (
    <Card bordered={false} className="market-strip">
      <div className="market-strip-body">
        <div className="market-strip-coins-grid">
          {(isMobile ? coins.slice(0, 3) : coins).map((coin) => {
            const ticker = data?.[coin.symbol];
            const price = ticker ? Number(ticker.last) : undefined;
            const high = ticker?.high24h ? Number(ticker.high24h) : undefined;
            const low = ticker?.low24h ? Number(ticker.low24h) : undefined;
            const windowLabel = ticker?.bar ? ticker.bar.toUpperCase() : undefined;
            const mid = high && low ? (high + low) / 2 : undefined;
            const change = price && mid ? ((price - mid) / mid) * 100 : undefined;
            return (
              <div key={coin.symbol} className="market-strip-coin-card">
                <div className="market-strip-coin-meta">
                  <img
                    src={coin.icon}
                    alt={coin.name}
                    className="market-strip-coin-icon"
                    style={{ borderColor: coin.color }}
                  />
                  <Typography.Text className="market-strip-coin-name">{coin.name}</Typography.Text>
                </div>
                <Typography.Text className="market-strip-coin-price market-strip-number">
                  {formatNumber(price)}
                </Typography.Text>
                <Typography.Text
                  className={`market-strip-coin-change ${
                    change === undefined ? '' : change >= 0 ? 'positive' : 'negative'
                  }`}
                  aria-label={windowLabel ? `${windowLabel} 变动` : '区间变动'}
                >
                  {change !== undefined ? `${change >= 0 ? '+' : ''}${change.toFixed(2)}%` : '--'}
                </Typography.Text>
              </div>
            );
          })}
        </div>
        <div className="market-strip-gap" aria-hidden />
        <div className="market-strip-funds-panel">{initialChip}</div>
      </div>
    </Card>
  );
};

export default MarketStrip;
