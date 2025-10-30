import type { OrderItem } from '@api/types';
import { Card, Table } from 'antd';
import dayjs from 'dayjs';

interface Props {
  orders?: OrderItem[];
  loading?: boolean;
}

const columns = [
  {
    title: '时间',
    dataIndex: 'createdAt',
    key: 'createdAt',
    render: (value: string) => dayjs(Number(value)).format('MM-DD HH:mm:ss')
  },
  { title: '方向', dataIndex: 'side', key: 'side' },
  { title: '类型', dataIndex: 'type', key: 'type' },
  { title: '价格', dataIndex: 'price', key: 'price' },
  { title: '数量', dataIndex: 'size', key: 'size' },
  { title: '成交数量', dataIndex: 'filledSize', key: 'filledSize' },
  { title: '状态', dataIndex: 'status', key: 'status' }
];

const OrderHistoryTable = ({ orders, loading }: Props) => (
  <Card title="历史订单" bordered={false} loading={loading}>
    <Table rowKey="orderId" dataSource={orders} columns={columns} pagination={{ pageSize: 20 }} size="small" />
  </Card>
);

export default OrderHistoryTable;
