import type { PositionHistoryItem } from '@api/types';
import { Card, Table, Tag } from 'antd';
import type { ColumnsType } from 'antd/es/table';
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
  const parsed = dayjs(value);
  return parsed.isValid() ? parsed.format('MM-DD HH:mm:ss') : value;
};

const readMetadataNumber = (metadata: PositionHistoryItem['metadata'], key: string) => {
  if (!metadata || typeof metadata !== 'object' || Array.isArray(metadata)) {
    return undefined;
  }
  const raw = metadata[key];
  if (raw === null || raw === undefined) {
    return undefined;
  }
  if (typeof raw === 'number') {
    return Number.isFinite(raw) ? raw : undefined;
  }
  if (typeof raw === 'string') {
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : undefined;
  }
  return undefined;
};

const columns: ColumnsType<PositionHistoryItem> = [
  {
    title: '合约',
    dataIndex: 'instId',
    key: 'instId'
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
    dataIndex: 'size',
    key: 'size',
    render: (value?: number) => formatNumber(value, 4)
  },
  {
    title: '开仓价',
    dataIndex: 'avgPrice',
    key: 'avgPrice',
    render: (value?: number) => formatNumber(value)
  },
  {
    title: '平仓价',
    key: 'exit_price',
    render: (_: unknown, record) => {
      if (record.closedAt) {
        const exitPx =
          readMetadataNumber(record.metadata, 'closePx') ??
          readMetadataNumber(record.metadata, 'last') ??
          record.markPx;
        return formatNumber(exitPx);
      }
      return formatNumber(record.markPx);
    }
  },
  {
    title: '盈亏',
    dataIndex: 'unrealizedPnl',
    key: 'unrealizedPnl',
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
    title: '平仓动作',
    dataIndex: 'actionKind',
    key: 'actionKind',
    render: (value?: string) => value ?? '-'
  },
  {
    title: '开仓时间',
    dataIndex: 'lastTradeAt',
    key: 'lastTradeAt',
    render: (value?: string) => formatTimestamp(value)
  },
  {
    title: '平仓时间',
    dataIndex: 'closedAt',
    key: 'closedAt',
    render: (value?: string) => formatTimestamp(value)
  }
];

const PositionsHistoryTable = ({ history, loading, embedded }: Props) => {
  const table = (
    <Table
      rowKey={(record) => `${record.instId}-${record.updatedAt}`}
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
