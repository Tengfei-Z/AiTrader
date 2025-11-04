import { Card, Tabs } from 'antd';
import type { PositionHistoryItem, PositionItem, StrategyMessage } from '@api/types';
import PositionsTable from '@components/PositionsTable';
import PositionsHistoryTable from '@components/PositionsHistoryTable';
import StrategyChatCard from '@components/StrategyChatCard';

interface Props {
  positions?: PositionItem[];
  history?: PositionHistoryItem[];
  strategyMessages?: StrategyMessage[];
  positionsLoading?: boolean;
  historyLoading?: boolean;
  strategyLoading?: boolean;
  onStrategyRefresh?: () => void;
  onStrategyStart?: () => void;
  strategyRunning?: boolean;
  className?: string;
}

const PositionsHistoryCard = ({
  positions,
  history,
  strategyMessages,
  positionsLoading,
  historyLoading,
  strategyLoading,
  onStrategyRefresh,
  onStrategyStart,
  strategyRunning,
  className
}: Props) => {
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
    },
    {
      key: 'strategy',
      label: '策略对话',
      children: (
        <StrategyChatCard
          messages={strategyMessages}
          loading={strategyLoading}
          onRefresh={onStrategyRefresh}
          onStart={onStrategyStart}
          starting={strategyRunning}
          embedded
        />
      )
    }
  ];

  return (
    <Card bordered={false} className={className}>
      <Tabs items={items} defaultActiveKey="positions" />
    </Card>
  );
};

export default PositionsHistoryCard;
