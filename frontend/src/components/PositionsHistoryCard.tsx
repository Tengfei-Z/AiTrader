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
  onStrategyStart?: () => void;
  strategyRunning?: boolean;
  manualTriggerEnabled?: boolean;
  className?: string;
}

const PositionsHistoryCard = ({
  positions,
  history,
  strategyMessages,
  positionsLoading,
  historyLoading,
  strategyLoading,
  onStrategyStart,
  strategyRunning,
  manualTriggerEnabled,
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

  const positionItems = [
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
    }
  ];

  return (
    <div className={[className, 'positions-history-card'].filter(Boolean).join(' ')}>
      <Card bordered={false} className="positions-history-card__block">
        <Tabs items={positionItems} defaultActiveKey="positions" />
      </Card>
      <Card bordered={false} className="positions-history-card__block positions-history-card__block--strategy">
        <StrategyChatCard
          messages={strategyMessages}
          loading={strategyLoading}
          onStart={onStrategyStart}
          starting={strategyRunning}
          manualTriggerEnabled={manualTriggerEnabled}
          embedded
        />
      </Card>
    </div>
  );
};

export default PositionsHistoryCard;
