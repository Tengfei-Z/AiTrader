import type { FillItem } from '@api/types';
import { Card, Table, Tag } from 'antd';
import dayjs from 'dayjs';

interface Props {
  fills?: FillItem[];
  loading?: boolean;
  embedded?: boolean;
}

const formatNumber = (value: string, fractionDigits = 2) =>
  Number(value).toLocaleString(undefined, { maximumFractionDigits: fractionDigits });

const columns = [
  {
    title: '时间',
    dataIndex: 'timestamp',
    key: 'timestamp',
    render: (value: string) => dayjs(Number(value)).format('MM-DD HH:mm:ss')
  },
  {
    title: '合约',
    dataIndex: 'symbol',
    key: 'symbol'
  },
  {
    title: '方向',
    dataIndex: 'side',
    key: 'side',
    render: (value: string) => <Tag color={value === 'buy' ? 'green' : 'volcano'}>{value}</Tag>
  },
  {
    title: '价格',
    dataIndex: 'price',
    key: 'price',
    render: (value: string) => formatNumber(value)
  },
  {
    title: '数量',
    dataIndex: 'size',
    key: 'size',
    render: (value: string) => formatNumber(value, 4)
  },
  {
    title: '手续费',
    dataIndex: 'fee',
    key: 'fee',
    render: (value: string) => formatNumber(value)
  },
  {
    title: '已实现盈亏',
    dataIndex: 'pnl',
    key: 'pnl',
    render: (value: string | undefined) =>
      value ? (
        <span style={{ color: Number(value) >= 0 ? '#16a34a' : '#dc2626' }}>
          {Number(value).toLocaleString(undefined, { maximumFractionDigits: 2 })}
        </span>
      ) : (
        '-'
      )
  }
];

const FillsTable = ({ fills, loading, embedded }: Props) => {
  const table = (
    <Table
      rowKey="fillId"
      dataSource={fills}
      columns={columns}
      pagination={{ pageSize: 20 }}
      size="small"
      loading={loading}
    />
  );

  if (embedded) {
    return table;
  }

  return (
    <Card title="历史订单" bordered={false} loading={loading}>
      {table}
    </Card>
  );
};

export default FillsTable;
