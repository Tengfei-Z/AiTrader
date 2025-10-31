import type { FillItem, OrderItem } from '@api/types';
import { Badge, Card, Col, Flex, Row, Statistic, Typography } from 'antd';
import dayjs from 'dayjs';
import { useMemo } from 'react';

interface Props {
  symbol: string;
  fills?: FillItem[];
  openOrders?: OrderItem[];
  loading?: boolean;
}

const AutomationStatusCard = ({ symbol, fills, openOrders, loading }: Props) => {
  const metrics = useMemo(() => {
    if (!fills || fills.length === 0) {
      return {
        lastFillText: '暂无成交记录',
        totalTrades24h: 0,
        totalVolume24h: 0,
        netVolume24h: 0,
        avgPrice24h: undefined as number | undefined
      };
    }

    const sorted = [...fills].sort((a, b) => Number(b.timestamp) - Number(a.timestamp));
    const lastFill = sorted[0];
    const now = Date.now();
    const range = 24 * 60 * 60 * 1000;
    const recent = sorted.filter((item) => now - Number(item.timestamp) <= range);

    const totalVolume24h = recent.reduce((total, item) => total + Number(item.size), 0);
    const buyVolume24h = recent.reduce((total, item) => (item.side === 'buy' ? total + Number(item.size) : total), 0);
    const sellVolume24h = recent.reduce((total, item) => (item.side === 'sell' ? total + Number(item.size) : total), 0);
    const grossNotional24h = recent.reduce(
      (total, item) => total + Number(item.price) * Number(item.size),
      0
    );

    return {
      lastFillText: `${dayjs(Number(lastFill.timestamp)).format('MM-DD HH:mm')} · ${
        lastFill.side === 'buy' ? '买入' : '卖出'
      } ${Number(lastFill.size).toLocaleString()} @ ${Number(lastFill.price).toLocaleString()}`,
      totalTrades24h: recent.length,
      totalVolume24h,
      netVolume24h: buyVolume24h - sellVolume24h,
      avgPrice24h: totalVolume24h > 0 ? grossNotional24h / totalVolume24h : undefined
    };
  }, [fills]);

  return (
    <Card title="AI 执行状态" bordered={false} loading={loading}>
      <Flex vertical gap={16}>
        <Flex align="center" gap={12}>
          <Badge status="processing" text="运行中" />
          <Typography.Text type="secondary">策略实时监控中</Typography.Text>
        </Flex>
        <div>
          <Typography.Text type="secondary">当前交易对</Typography.Text>
          <Typography.Title level={4} style={{ margin: '4px 0 0' }}>
            {symbol}
          </Typography.Title>
        </div>
        <Row gutter={16}>
          <Col span={12}>
            <Statistic title="未完成委托" value={openOrders?.length ?? 0} />
          </Col>
          <Col span={12}>
            <Statistic title="24h 成交笔数" value={metrics.totalTrades24h} />
          </Col>
        </Row>
        <Row gutter={16}>
          <Col span={12}>
            <Statistic
              title="24h 成交量"
              value={
                metrics.totalVolume24h > 0
                  ? Number(metrics.totalVolume24h.toFixed(4))
                  : metrics.totalTrades24h > 0
                    ? 0
                    : '--'
              }
            />
          </Col>
          <Col span={12}>
            <Statistic
              title="24h 净方向"
              value={
                metrics.totalTrades24h > 0
                  ? Number(metrics.netVolume24h.toFixed(4))
                  : '--'
              }
            />
          </Col>
        </Row>
        <Row gutter={16}>
          <Col span={12}>
            <Statistic
              title="24h 均价(估算)"
              value={metrics.avgPrice24h !== undefined ? metrics.avgPrice24h.toFixed(2) : '--'}
              suffix={metrics.avgPrice24h !== undefined ? 'USDT' : undefined}
            />
          </Col>
          <Col span={12}>
            <Statistic
              title="最新成交"
              value={metrics.lastFillText}
              valueStyle={{ fontSize: 14, whiteSpace: 'normal' }}
            />
          </Col>
        </Row>
      </Flex>
    </Card>
  );
};

export default AutomationStatusCard;
