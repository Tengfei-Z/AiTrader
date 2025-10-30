import { Card } from 'antd';
import { Area, AreaChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';

interface Props {
  data: { time: string; price: number }[];
  loading?: boolean;
}

const PriceChart = ({ data, loading }: Props) => (
  <Card title="价格走势" bordered={false} loading={loading}>
    <div style={{ width: '100%', height: 320 }}>
      <ResponsiveContainer>
        <AreaChart data={data}>
          <defs>
            <linearGradient id="priceGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="#38bdf8" stopOpacity={0.8} />
              <stop offset="95%" stopColor="#38bdf8" stopOpacity={0} />
            </linearGradient>
          </defs>
          <XAxis dataKey="time" hide tick={{ fill: '#94a3b8' }} />
          <YAxis domain={['dataMin', 'dataMax']} tick={{ fill: '#94a3b8' }} />
          <Tooltip
            contentStyle={{ background: '#1f2937', border: 'none' }}
            labelStyle={{ color: '#e2e8f0' }}
            formatter={(value: number) => value.toLocaleString()}
          />
          <Area type="monotone" dataKey="price" stroke="#38bdf8" fill="url(#priceGradient)" strokeWidth={2} />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  </Card>
);

export default PriceChart;
