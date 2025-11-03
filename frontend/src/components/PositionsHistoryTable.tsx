import type { PositionHistoryItem } from '@api/types';
import { Card, Table, Tag } from 'antd';
import dayjs from 'dayjs';

interface Props {
  history?: PositionHistoryItem[];
  loading?: boolean;
  embedded?: boolean;
}

const formatNumber = (value?: number, fractionDigits = 2) =>
  value !== undefined
    ? Number(value).toLocaleString(undefined, {
        maximumFractionDigits: fractionDigits
      })
    : '-';

const formatTimestamp = (value?: string) => {
  if (!value) {
    return '-';
  }
  const timestamp = Number(value);
  if (Number.isFinite(timestamp)) {
    return dayjs(timestamp).format('MM-DD HH:mm:ss');
  }
  return value;
};

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
    render: (value: string) => {
      const label = value === 'long' ? '做多' : value === 'short' ? '做空' : '净持仓';
      return (
        <Tag color={value === 'long' ? 'green' : value === 'short' ? 'volcano' : 'blue'}>
          {label}
        </Tag>
      );
    }
  },
  {
    title: '数量',
    dataIndex: 'quantity',
    key: 'quantity',
    render: (value?: number) => formatNumber(value, 4)
  },
  {
    title: '杠杆',
    dataIndex: 'leverage',
    key: 'leverage',
    render: (value?: number) => (value ? `${formatNumber(value, 2)}x` : '-')
  },
  {
    title: '开仓价',
    dataIndex: 'entry_price',
    key: 'entry_price',
    render: (value?: number) => formatNumber(value)
  },
  {
    title: '平仓价',
    dataIndex: 'exit_price',
    key: 'exit_price',
    render: (value?: number) => formatNumber(value)
  },
  {
    title: '保证金',
    dataIndex: 'margin',
    key: 'margin',
    render: (value?: number) => formatNumber(value)
  },
  {
    title: '已实现盈亏',
    dataIndex: 'realized_pnl',
    key: 'realized_pnl',
    render: (value?: number) =>
      value !== undefined ? (
        <span style={{ color: value >= 0 ? '#16a34a' : '#dc2626' }}>
          {formatNumber(value)}
        </span>
      ) : (
        '-'
      )
  },
  {
    title: '开仓时间',
    dataIndex: 'entry_time',
    key: 'entry_time',
    render: (value?: string) => formatTimestamp(value)
  },
  {
    title: '平仓时间',
    dataIndex: 'exit_time',
    key: 'exit_time',
    render: (value?: string) => formatTimestamp(value)
  }
];

const PositionsHistoryTable = ({ history, loading, embedded }: Props) => {
  const table = (
    <Table
      rowKey={(record) => `${record.symbol}-${record.exit_time ?? record.entry_time ?? 'unknown'}`}
      dataSource={history ?? []}
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
    <Card title="历史持仓" bordered={false} loading={loading}>
      {table}
    </Card>
  );
};

export default PositionsHistoryTable;
