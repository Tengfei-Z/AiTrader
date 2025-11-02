import { Card, Flex, Radio, Typography } from 'antd';
import { useMemo, useState } from 'react';
import { ResponsiveContainer, LineChart, Line, CartesianGrid, XAxis, YAxis, Tooltip, ReferenceLine } from 'recharts';
import dayjs from 'dayjs';
import type { EquityPoint } from '@utils/pnl';

interface Props {
  data: EquityPoint[];
  loading?: boolean;
  className?: string;
}

const EquityCurveCard = ({ data, loading, className }: Props) => {
  const [range, setRange] = useState<'all' | '72h'>('all');
  const [metric, setMetric] = useState<'$' | '%'>('$');

  const slicedData = useMemo(() => {
    if (range === '72h' && data.length > 0) {
      const cutoff = Date.now() - 72 * 3600 * 1000;
      return data.filter((item) => Number(item.time) >= cutoff);
    }
    return data;
  }, [data, range]);

  const initialEquity = data.length > 0 ? data[0].equity : 0;
  const lastEquity = slicedData.length > 0
    ? slicedData[slicedData.length - 1].equity
    : initialEquity;

  const minValue = slicedData.length > 0
    ? Math.min(...slicedData.map((item) => item.equity))
    : initialEquity;
  const maxValue = slicedData.length > 0
    ? Math.max(...slicedData.map((item) => item.equity))
    : initialEquity;
  const padding = Math.max(Math.abs(maxValue - minValue) * 0.1, 10);
  const changeValue = lastEquity - initialEquity;
  const changePercent = initialEquity !== 0 ? (changeValue / initialEquity) * 100 : 0;
  const changeColor = changeValue >= 0 ? '#16a34a' : '#dc2626';

  return (
    <Card bordered={false} loading={loading} className={className}>
      <Flex justify="space-between" align="center" className="equity-header">
        <Flex align="center" gap={16}>
          <Radio.Group
            value={metric}
            onChange={(e) => setMetric(e.target.value)}
            optionType="button"
            buttonStyle="solid"
            size="small"
          >
            <Radio.Button value="$">$</Radio.Button>
            <Radio.Button value="%">%</Radio.Button>
          </Radio.Group>
          <div className="equity-stats">
            <div className="equity-stat">
              <Typography.Text className="equity-label">收益</Typography.Text>
              <Typography.Text className="equity-value">
                {lastEquity.toLocaleString(undefined, { maximumFractionDigits: 2 })}
              </Typography.Text>
              <Typography.Text className="equity-delta" style={{ color: changeColor }}>
                {metric === '$'
                  ? `${changeValue >= 0 ? '+' : ''}${changeValue.toLocaleString(undefined, {
                      maximumFractionDigits: 2
                    })}`
                  : `${changePercent >= 0 ? '+' : ''}${changePercent.toFixed(2)}%`}
              </Typography.Text>
            </div>
          </div>
        </Flex>
        <Radio.Group
          value={range}
          onChange={(e) => setRange(e.target.value)}
          optionType="button"
          buttonStyle="solid"
          size="small"
        >
          <Radio.Button value="all">ALL</Radio.Button>
          <Radio.Button value="72h">72H</Radio.Button>
        </Radio.Group>
      </Flex>
      <div className="chart-container">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={slicedData}>
            <CartesianGrid strokeDasharray="3 3" stroke="#e5e7eb" />
            <XAxis
              dataKey="time"
              tickFormatter={(value) => dayjs(Number(value)).format('MM-DD HH:mm')}
              stroke="#94a3b8"
              minTickGap={32}
            />
            <YAxis
              stroke="#94a3b8"
              tickFormatter={(value) => value.toLocaleString(undefined, { maximumFractionDigits: 2 })}
              domain={[minValue - padding, maxValue + padding]}
            />
            <ReferenceLine y={0} stroke="#9ca3af" strokeDasharray="4 4" />
            <Tooltip
              formatter={(value: number) => value.toLocaleString(undefined, { maximumFractionDigits: 2 })}
              labelFormatter={(label) => dayjs(Number(label)).format('YYYY-MM-DD HH:mm')}
            />
            <Line
              type="monotone"
              dataKey="equity"
              name="收益"
              stroke="#4f46e5"
              strokeWidth={3}
              dot={false}
              activeDot={{ r: 6 }}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </Card>
  );
};

export default EquityCurveCard;
