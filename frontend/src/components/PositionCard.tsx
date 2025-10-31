import { Card, Col, Flex, Row, Statistic, Tag, Typography } from 'antd';

interface Props {
  symbol: string;
  netPosition: number;
  avgEntryPrice?: number;
  markPrice?: number;
  unrealizedPnl?: number;
}

const formatNumber = (value?: number, fractionDigits = 2) =>
  value !== undefined
    ? value.toLocaleString(undefined, {
        maximumFractionDigits: fractionDigits,
        minimumFractionDigits: value >= 1000 ? 0 : Math.min(fractionDigits, 2)
      })
    : '--';

const PositionCard = ({ symbol, netPosition, avgEntryPrice, markPrice, unrealizedPnl }: Props) => {
  const positionAbs = Math.abs(netPosition);
  const side = netPosition > 0 ? '多头' : netPosition < 0 ? '空头' : '空仓';
  const sideColor = netPosition > 0 ? 'green' : netPosition < 0 ? 'red' : 'default';
  const base = symbol.split('-')[0];
  const notional = markPrice !== undefined ? positionAbs * markPrice : undefined;
  const entryLabel =
    avgEntryPrice !== undefined ? `${formatNumber(avgEntryPrice)} USDT` : netPosition === 0 ? '--' : '计算中';
  const currentLabel = markPrice !== undefined ? `${formatNumber(markPrice)} USDT` : '--';
  const pnlColor = unrealizedPnl !== undefined && unrealizedPnl >= 0 ? '#16a34a' : '#dc2626';

  return (
    <Card title="当前持仓" bordered={false}>
      <Flex vertical gap={16}>
        <Flex align="center" gap={12}>
          <Typography.Title level={4} style={{ margin: 0 }}>
            {symbol}
          </Typography.Title>
          <Tag color={sideColor}>{side}</Tag>
        </Flex>
        <Row gutter={[24, 24]}>
          <Col xs={12} md={6}>
            <Statistic title={`持仓数量 (${base})`} value={formatNumber(positionAbs, 4)} />
          </Col>
          <Col xs={12} md={6}>
            <Statistic title="持仓名义价值" value={notional !== undefined ? formatNumber(notional) : '--'} suffix="USDT" />
          </Col>
          <Col xs={12} md={6}>
            <Statistic title="开仓均价" value={entryLabel} />
          </Col>
          <Col xs={12} md={6}>
            <Statistic title="最新价格" value={currentLabel} />
          </Col>
        </Row>
        <Row gutter={[24, 24]}>
          <Col xs={12} md={6}>
            <Statistic
              title="未实现盈亏"
              value={
                unrealizedPnl !== undefined
                  ? unrealizedPnl.toLocaleString(undefined, { maximumFractionDigits: 2 })
                  : '--'
              }
              suffix={unrealizedPnl !== undefined ? 'USDT' : undefined}
              valueStyle={{ color: unrealizedPnl !== undefined ? pnlColor : undefined }}
            />
          </Col>
        </Row>
      </Flex>
    </Card>
  );
};

export default PositionCard;
