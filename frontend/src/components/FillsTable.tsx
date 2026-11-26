import type { FillItem, OrderItem } from '@api/types';
import { Card, Table, Tag } from 'antd';
import dayjs from 'dayjs';
import { inferFillActionColor, inferFillActionLabel } from '@utils/tradeLabels';

interface Props {
  fills?: FillItem[];
  openOrders?: OrderItem[];
  loading?: boolean;
  embedded?: boolean;
}

const formatNumber = (value: string, fractionDigits = 2) =>
  Number(value).toLocaleString(undefined, { maximumFractionDigits: fractionDigits });

const buildColumns = (openOrders?: OrderItem[]) => [
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
    render: (_value: string, record: FillItem) => (
      <Tag color={inferFillActionColor(record, openOrders)}>{inferFillActionLabel(record, openOrders)}</Tag>
    )
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

const FillsTable = ({ fills, openOrders, loading, embedded }: Props) => {
  const columns = buildColumns(openOrders);
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
