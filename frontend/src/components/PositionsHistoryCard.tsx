import { Card, Tabs } from 'antd';
import type { PositionHistoryItem, PositionItem } from '@api/types';
import PositionsTable from '@components/PositionsTable';
import PositionsHistoryTable from '@components/PositionsHistoryTable';

interface Props {
  positions?: PositionItem[];
  history?: PositionHistoryItem[];
  positionsLoading?: boolean;
  historyLoading?: boolean;
  className?: string;
}

const PositionsHistoryCard = ({ positions, history, positionsLoading, historyLoading, className }: Props) => {
  const items = [
    {
      key: 'positions',
      label: '当前持仓',
      children: <PositionsTable positions={positions} loading={positionsLoading} embedded />
    },
    {
      key: 'history',
      label: '历史持仓',
      children: <PositionsHistoryTable history={history} loading={historyLoading} embedded />
    }
  ];

  return (
    <Card bordered={false} className={className}>
      <Tabs items={items} defaultActiveKey="positions" />
    </Card>
  );
};

export default PositionsHistoryCard;
