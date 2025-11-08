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
    dataIndex: 'symbol',
    key: 'symbol'
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
  },
  {
    title: '止盈',
    key: 'take_profit',
    render: (_: unknown, record) =>
      renderExitPlan(record.take_profit_trigger, record.take_profit_price, record.take_profit_type)
  },
  {
    title: '止损',
    key: 'stop_loss',
    render: (_: unknown, record) =>
      renderExitPlan(record.stop_loss_trigger, record.stop_loss_price, record.stop_loss_type)
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
