import { Card, Tabs } from 'antd';
import type { PositionHistoryItem, PositionItem, StrategyMessage } from '@api/types';
import PositionsTable from '@components/PositionsTable';
import PositionsHistoryTable from '@components/PositionsHistoryTable';
import StrategyChatCard from '@components/StrategyChatCard';
import type { ReactNode } from 'react';

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
  const wrapTabContent = (node: ReactNode, scrollable?: boolean) => (
    <div
      className={[
        'positions-history-card__pane',
        scrollable ? 'positions-history-card__pane--scrollable' : ''
      ]
        .filter(Boolean)
        .join(' ')}
    >
      {node}
    </div>
  );

  const items = [
    {
      key: 'positions',
      label: '当前持仓',
      children: wrapTabContent(
        <PositionsTable positions={positions} loading={positionsLoading} embedded />,
        true
      )
    },
    {
      key: 'history',
      label: '历史持仓',
      children: wrapTabContent(
        <PositionsHistoryTable history={history} loading={historyLoading} embedded />,
        true
      )
    },
    {
      key: 'strategy',
      label: '策略对话',
      children: wrapTabContent(
        <StrategyChatCard
          messages={strategyMessages}
          loading={strategyLoading}
          onRefresh={onStrategyRefresh}
          onStart={onStrategyStart}
          starting={strategyRunning}
          embedded
        />,
        false
      )
    }
  ];

  return (
    <Card
      bordered={false}
      className={[className, 'positions-history-card'].filter(Boolean).join(' ')}
    >
      <Tabs items={items} defaultActiveKey="positions" />
    </Card>
  );
};

export default PositionsHistoryCard;
