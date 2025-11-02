import { Card, Tabs } from 'antd';
import type { PositionItem, FillItem } from '@api/types';
import PositionsTable from '@components/PositionsTable';
import FillsTable from '@components/FillsTable';

interface Props {
  positions?: PositionItem[];
  fills?: FillItem[];
  positionsLoading?: boolean;
  fillsLoading?: boolean;
  className?: string;
}

const PositionsHistoryCard = ({ positions, fills, positionsLoading, fillsLoading, className }: Props) => {
  const items = [
    {
      key: 'positions',
      label: '当前持仓',
      children: <PositionsTable positions={positions} loading={positionsLoading} embedded />
    },
    {
      key: 'fills',
      label: '历史订单',
      children: <FillsTable fills={fills} loading={fillsLoading} embedded />
    }
  ];

  return (
    <Card bordered={false} className={className}>
      <Tabs items={items} defaultActiveKey="positions" />
    </Card>
  );
};

export default PositionsHistoryCard;
