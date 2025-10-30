import type { BalanceItem } from '@api/types';
import { Card, Table } from 'antd';

interface Props {
  balances?: BalanceItem[];
  loading?: boolean;
}

const columns = [
  { title: '资产', dataIndex: 'asset', key: 'asset' },
  { title: '可用', dataIndex: 'available', key: 'available' },
  { title: '冻结', dataIndex: 'locked', key: 'locked' },
  {
    title: '折合 USDT',
    dataIndex: 'valuationUSDT',
    key: 'valuationUSDT',
    render: (value: string | undefined) => value ?? '-'
  }
];

const BalancesTable = ({ balances, loading }: Props) => (
  <Card title="账户余额" bordered={false} loading={loading}>
    <Table rowKey="asset" dataSource={balances} columns={columns} pagination={false} size="small" />
  </Card>
);

export default BalancesTable;
