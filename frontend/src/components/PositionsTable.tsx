import type { PositionItem } from '@api/types';
import { Card, Table, Tag } from 'antd';
import type { ColumnsType } from 'antd/es/table';

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

const readMetadataNumber = (metadata: PositionItem['metadata'], key: string) => {
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

const readMetadataString = (metadata: PositionItem['metadata'], key: string) => {
  if (!metadata || typeof metadata !== 'object' || Array.isArray(metadata)) {
    return undefined;
  }
  const raw = metadata[key];
  return typeof raw === 'string' ? raw : undefined;
};

const renderExitPlan = (trigger?: number, order?: number, triggerType?: string) => {
  if (trigger === undefined && order === undefined) {
    return '-';
  }
  const parts: string[] = [];
  if (trigger !== undefined) {
    parts.push(`触发 ${formatNumber(trigger)}`);
  }
  if (order !== undefined) {
    parts.push(`委托 ${formatNumber(order)}`);
  }
  if (triggerType) {
    parts.push(triggerType.toUpperCase());
  }
  return parts.join(' / ');
};

const columns: ColumnsType<PositionItem> = [
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
      return <Tag color={value === 'long' ? 'green' : value === 'short' ? 'volcano' : 'blue'}>{label}</Tag>;
    }
  },
  {
    title: '数量',
    dataIndex: 'size',
    key: 'size',
    render: (value: number | undefined) => formatNumber(value, 4)
  },
  {
    title: '杠杆',
    key: 'leverage',
    render: (_: unknown, record) => {
      const leverage = readMetadataNumber(record.metadata, 'lever');
      return leverage ? `${formatNumber(leverage, 2)}x` : '-';
    }
  },
  {
    title: '开仓价',
    dataIndex: 'avgPrice',
    key: 'avgPrice',
    render: (value: number | undefined) => formatNumber(value)
  },
  {
    title: '当前价',
    dataIndex: 'markPx',
    key: 'markPx',
    render: (value: number | undefined) => formatNumber(value)
  },
  {
    title: '未实现盈亏',
    dataIndex: 'unrealizedPnl',
    key: 'unrealizedPnl',
    render: (value: number | undefined) => formatNumber(value)
  },
  {
    title: '止盈',
    key: 'take_profit',
    render: (_: unknown, record) =>
      renderExitPlan(
        readMetadataNumber(record.metadata, 'tpTriggerPx'),
        readMetadataNumber(record.metadata, 'tpOrdPx'),
        readMetadataString(record.metadata, 'tpTriggerPxType')
      )
  },
  {
    title: '止损',
    key: 'stop_loss',
    render: (_: unknown, record) =>
      renderExitPlan(
        readMetadataNumber(record.metadata, 'slTriggerPx'),
        readMetadataNumber(record.metadata, 'slOrdPx'),
        readMetadataString(record.metadata, 'slTriggerPxType')
      )
  }
];

const PositionsTable = ({ positions, loading, embedded }: Props) => {
  const table = (
    <Table
      rowKey={(record) => `${record.instId}-${record.side}-${record.updatedAt}`}
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
