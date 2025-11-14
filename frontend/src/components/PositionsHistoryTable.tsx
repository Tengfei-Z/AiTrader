import type { PositionHistoryItem } from '@api/types';
import { Card, Table, Tag, Grid } from 'antd';
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

const renderSideTag = (value: string) => {
  const label = value === 'long' ? '做多' : value === 'short' ? '做空' : '净持仓';
  const color = value === 'long' ? 'green' : value === 'short' ? 'volcano' : 'blue';
  return (
    <Tag color={color}>
      {label}
    </Tag>
  );
};

const desktopColumns: ColumnsType<PositionHistoryItem> = [
  {
    title: '合约',
    dataIndex: 'instId',
    key: 'instId'
  },
  {
    title: '方向',
    dataIndex: 'side',
    key: 'side',
    render: (value: string) => renderSideTag(value)
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
  const screens = Grid.useBreakpoint();
  const isMobile = !screens.md;

  const mobileColumns: ColumnsType<PositionHistoryItem> = [
    {
      title: '历史持仓',
      key: 'mobile',
      render: (_: unknown, record) => {
        const exitLabel =
          record.closedAt && record.metadata
            ? formatNumber(
                readMetadataNumber(record.metadata, 'closePx') ??
                  readMetadataNumber(record.metadata, 'last') ??
                  record.markPx
              )
            : formatNumber(record.markPx);
        const pnl = record.unrealizedPnl;

        return (
          <div className="table-mobile-card table-mobile-card--compact">
            <div className="table-mobile-card__header">
              <div>
                <span className="table-mobile-card__title">{record.instId}</span>
              </div>
              {renderSideTag(record.side)}
            </div>
            <div className="table-mobile-card__meta">
              <span>开仓 {formatNumber(record.avgPrice)}</span>
              <span>平仓 {exitLabel}</span>
            </div>
            <div className="table-mobile-card__footer">
              <span className="table-mobile-card__label">盈亏</span>
              <span
                className={`table-mobile-card__value ${
                  pnl === undefined ? '' : pnl >= 0 ? 'positive' : 'negative'
                }`}
              >
                {formatNumber(pnl)}
              </span>
            </div>
            <div className="table-mobile-card__timestamps">
              <span>开 {formatTimestamp(record.lastTradeAt)}</span>
              <span>平 {formatTimestamp(record.closedAt)}</span>
            </div>
          </div>
        );
      }
    }
  ];

  const columns = isMobile ? mobileColumns : desktopColumns;

  const table = (
    <Table
      rowKey={(record) => `${record.instId}-${record.updatedAt}`}
      dataSource={history ?? []}
      columns={columns}
      className="positions-table positions-table--compact"
      pagination={{ pageSize: 20 }}
      size="small"
      loading={loading}
      scroll={isMobile ? undefined : { x: 900 }}
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
