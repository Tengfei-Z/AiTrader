import type { PositionItem } from '@api/types';
import { Card, Table, Tag } from 'antd';

interface Props {
  positions?: PositionItem[];
  loading?: boolean;
  embedded?: boolean;
}

const formatNumber = (value?: number, fractionDigits = 2) =>
  value !== undefined
    ? Number(value).toLocaleString(undefined, {
        maximumFractionDigits: fractionDigits
      })
    : '-';

const columns = [
  {
    title: '合约',
    dataIndex: 'symbol',
    key: 'symbol'
  },
  {
    title: '方向',
    dataIndex: 'side',
    key: 'side',
    render: (value: string) => (
      <Tag color={value === 'long' ? 'green' : 'volcano'}>{value}</Tag>
    )
  },
  {
    title: '数量',
    dataIndex: 'quantity',
    key: 'quantity',
    render: (value: number | undefined) => formatNumber(value, 4)
  },
  {
    title: '杠杆',
    dataIndex: 'leverage',
    key: 'leverage',
    render: (value: number | undefined) => (value ? `${formatNumber(value, 2)}x` : '-')
  },
  {
    title: '开仓价',
    dataIndex: 'entry_price',
    key: 'entry_price',
    render: (value: number | undefined) => formatNumber(value)
  },
  {
    title: '当前价',
    dataIndex: 'current_price',
    key: 'current_price',
    render: (value: number | undefined) => formatNumber(value)
  },
  {
    title: '未实现盈亏',
    dataIndex: 'unrealized_pnl',
    key: 'unrealized_pnl',
    render: (value: number | undefined) => formatNumber(value)
  },
  {
    title: '强平价',
    dataIndex: 'liquidation_price',
    key: 'liquidation_price',
    render: (value: number | undefined) => formatNumber(value)
  },
  {
    title: '保证金',
    dataIndex: 'margin',
    key: 'margin',
    render: (value: number | undefined) => formatNumber(value)
  }
];

const PositionsTable = ({ positions, loading, embedded }: Props) => {
  const table = (
    <Table
      rowKey={(record) => `${record.symbol}-${record.side}-${record.entry_time ?? 'na'}`}
      dataSource={positions ?? []}
      columns={columns}
      pagination={false}
      size="small"
      loading={loading}
    />
  );

  if (embedded) {
    return table;
  }

  return (
    <Card title="当前持仓" bordered={false} loading={loading}>
      {table}
    </Card>
  );
};

export default PositionsTable;
