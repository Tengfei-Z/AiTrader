import type { FillItem } from '@api/types';
import { Card, Table } from 'antd';
import dayjs from 'dayjs';

interface Props {
  fills?: FillItem[];
  loading?: boolean;
}

const columns = [
  {
    title: '时间',
    dataIndex: 'timestamp',
    key: 'timestamp',
    render: (value: string) => dayjs(Number(value)).format('MM-DD HH:mm:ss')
  },
  { title: '方向', dataIndex: 'side', key: 'side' },
  { title: '成交价', dataIndex: 'price', key: 'price' },
  { title: '数量', dataIndex: 'size', key: 'size' },
  { title: '手续费', dataIndex: 'fee', key: 'fee' }
];

const FillsTable = ({ fills, loading }: Props) => (
  <Card title="成交记录" bordered={false} loading={loading}>
    <Table rowKey="fillId" size="small" pagination={{ pageSize: 20 }} dataSource={fills} columns={columns} />
  </Card>
);

export default FillsTable;
