import { Card, Table } from 'antd';
import type { TradeItem } from '@api/types';
import dayjs from 'dayjs';

interface Props {
  trades?: TradeItem[];
  loading?: boolean;
}

const columns = [
  {
    title: '时间',
    dataIndex: 'timestamp',
    key: 'timestamp',
    render: (value: string) => dayjs(Number(value)).format('HH:mm:ss')
  },
  {
    title: '方向',
    dataIndex: 'side',
    key: 'side',
    render: (value: TradeItem['side']) => (
      <span style={{ color: value === 'buy' ? '#16a34a' : '#dc2626' }}>{value === 'buy' ? '买入' : '卖出'}</span>
    )
  },
  {
    title: '价格',
    dataIndex: 'price',
    key: 'price',
    render: (value: string) => Number(value).toLocaleString()
  },
  {
    title: '数量',
    dataIndex: 'size',
    key: 'size',
    render: (value: string) => Number(value).toLocaleString()
  }
];

const RecentTradesTable = ({ trades, loading }: Props) => (
  <Card title="最新成交" bordered={false} loading={loading}>
    <Table
      rowKey="tradeId"
      size="small"
      pagination={false}
      columns={columns}
      dataSource={trades}
      scroll={{ y: 280 }}
    />
  </Card>
);

export default RecentTradesTable;
