import { Card } from 'antd';
import { ResponsiveContainer, LineChart, Line, CartesianGrid, XAxis, YAxis, Tooltip, ReferenceLine } from 'recharts';
import dayjs from 'dayjs';
import type { EquityPoint } from '@utils/pnl';

interface Props {
  data: EquityPoint[];
  loading?: boolean;
}

const EquityCurveCard = ({ data, loading }: Props) => {
  const minValue = data.length > 0 ? Math.min(...data.map((item) => item.equity)) : 0;
  const maxValue = data.length > 0 ? Math.max(...data.map((item) => item.equity)) : 0;
  const padding = Math.max(Math.abs(maxValue - minValue) * 0.1, 10);

  return (
    <Card title="总收益曲线" bordered={false} loading={loading}>
      <div style={{ width: '100%', height: 420 }}>
        <ResponsiveContainer>
          <LineChart data={data}>
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
