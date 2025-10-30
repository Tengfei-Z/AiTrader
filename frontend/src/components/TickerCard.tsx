import { Card, Flex, Statistic, Tag, Typography } from 'antd';
import type { Ticker } from '@api/types';
import dayjs from 'dayjs';

interface Props {
  ticker?: Ticker;
  loading?: boolean;
}

const formatNumber = (value?: string) => {
  if (!value) return '-';
  return Number(value).toLocaleString(undefined, { maximumFractionDigits: 2 });
};

const TickerCard = ({ ticker, loading }: Props) => {
  const last = ticker ? Number(ticker.last) : undefined;
  const high = ticker?.high24h ? Number(ticker.high24h) : undefined;
  const low = ticker?.low24h ? Number(ticker.low24h) : undefined;
  const vol = ticker?.vol24h ? Number(ticker.vol24h) : undefined;

  const bid = ticker?.bidPx ? Number(ticker.bidPx) : undefined;
  const isBull = bid !== undefined && last !== undefined ? last >= bid : false;

  return (
    <Card loading={loading} bordered={false} className="ticker-card">
      <Flex justify="space-between" align="center">
        <div>
          <Typography.Title level={3}>{ticker?.symbol ?? '--'}</Typography.Title>
          <Flex align="baseline" gap={12}>
            <Typography.Title level={2} style={{ color: isBull ? '#16a34a' : '#dc2626', margin: 0 }}>
              {last?.toLocaleString(undefined, { maximumFractionDigits: 2 }) ?? '--'}
            </Typography.Title>
            <Tag color={isBull ? 'green' : 'red'}>{isBull ? '上涨' : '下跌'}</Tag>
          </Flex>
          <Typography.Text type="secondary">
            更新时间：{ticker ? dayjs(Number(ticker.timestamp)).format('YYYY-MM-DD HH:mm:ss') : '--'}
          </Typography.Text>
        </div>
        <Flex gap={24}>
          <Statistic title="24h 最高" value={formatNumber(ticker?.high24h)} suffix="USD" />
          <Statistic title="24h 最低" value={formatNumber(ticker?.low24h)} suffix="USD" />
          <Statistic title="24h 成交量" value={formatNumber(ticker?.vol24h)} suffix="BTC" />
        </Flex>
      </Flex>
    </Card>
  );
};

export default TickerCard;
