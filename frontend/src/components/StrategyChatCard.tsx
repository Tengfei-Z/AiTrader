import type { ModelBudget, StrategyMessage } from '@api/types';
import { Card, Empty, Flex, List, Typography, Button, Spin, Modal, InputNumber } from 'antd';
import dayjs from 'dayjs';
import { useEffect, useState } from 'react';

interface Props {
  messages?: StrategyMessage[];
  loading?: boolean;
  className?: string;
  onStart?: () => void;
  starting?: boolean;
  embedded?: boolean;
  manualTriggerEnabled?: boolean;
  modelBudget?: ModelBudget;
  onBudgetLimitChange?: (limit: number) => Promise<void> | void;
  budgetLimitUpdating?: boolean;
}

const StrategyChatCard = ({
  messages,
  loading,
  className,
  onStart,
  starting,
  embedded,
  manualTriggerEnabled,
  modelBudget,
  onBudgetLimitChange,
  budgetLimitUpdating
}: Props) => {
  const [budgetModalOpen, setBudgetModalOpen] = useState(false);
  const [draftBudgetLimit, setDraftBudgetLimit] = useState<number | null>(
    modelBudget?.limit ?? null
  );
  const orderedMessages = (messages ?? [])
    .slice()
    .sort((a, b) => (b.createdAt ?? '').localeCompare(a.createdAt ?? ''));
  const spinning = Boolean(loading || starting);
  const allowManualTrigger = Boolean(manualTriggerEnabled);
  const remainingText = modelBudget && modelBudget.limit > 0 ? String(modelBudget.remaining) : '--';
  const limitText = modelBudget && modelBudget.limit > 0 ? String(modelBudget.limit) : '--';

  useEffect(() => {
    if (!budgetModalOpen) {
      setDraftBudgetLimit(modelBudget?.limit ?? null);
    }
  }, [budgetModalOpen, modelBudget?.limit]);

  const openBudgetModal = () => {
    if (!onBudgetLimitChange || !modelBudget?.limit) {
      return;
    }
    setDraftBudgetLimit(modelBudget.limit);
    setBudgetModalOpen(true);
  };

  const handleBudgetConfirm = async () => {
    if (!draftBudgetLimit || draftBudgetLimit <= 0) {
      return;
    }
    await onBudgetLimitChange?.(draftBudgetLimit);
    setBudgetModalOpen(false);
  };

  const content = (
    <Flex vertical gap={16} className="strategy-chat-panel">
      <Flex justify="space-between" align="center" className="strategy-chat-panel__toolbar">
        <Typography.Title level={5} className="strategy-chat-panel__title">
          策略对话
        </Typography.Title>
        <Flex align="center" gap={12} className="strategy-chat-panel__actions">
          <Typography.Text type="secondary" className="strategy-chat-panel__budget">
            剩余可触发大模型次数：{remainingText}/
            <Button
              type="link"
              size="small"
              className="strategy-chat-panel__budget-limit"
              onClick={openBudgetModal}
              disabled={!onBudgetLimitChange || !modelBudget?.limit}
            >
              {limitText}
            </Button>
          </Typography.Text>
          {onStart && allowManualTrigger && (
            <Button type="primary" size="small" onClick={onStart} loading={starting}>
              启动策略
            </Button>
          )}
        </Flex>
      </Flex>
      <Modal
        title="调整最大 AI 请求次数"
        open={budgetModalOpen}
        onOk={handleBudgetConfirm}
        onCancel={() => setBudgetModalOpen(false)}
        confirmLoading={budgetLimitUpdating}
        okText="保存"
        cancelText="取消"
        destroyOnClose
      >
        <InputNumber
          min={1}
          max={100000}
          precision={0}
          value={draftBudgetLimit}
          onChange={(value) => setDraftBudgetLimit(value)}
          className="strategy-chat-panel__budget-input"
          addonAfter="次/天"
        />
      </Modal>
      {orderedMessages.length === 0 ? (
        <Empty description="暂无对话记录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      ) : (
        <div className="strategy-chat-history">
          <List
            dataSource={orderedMessages}
            renderItem={(item) => (
              <List.Item className="strategy-chat-message">
                <Flex vertical gap={6}>
                  <Flex align="center" gap={12} className="strategy-chat-message__meta">
                    <Typography.Text type="secondary">
                      {dayjs(item.createdAt).format('MM-DD HH:mm')}
                    </Typography.Text>
                    <Typography.Text type="secondary">会话 {item.sessionId}</Typography.Text>
                  </Flex>
                  <Typography.Paragraph
                    className="strategy-chat-message__content"
                  >
                    {item.summary}
                  </Typography.Paragraph>
                </Flex>
              </List.Item>
            )}
          />
        </div>
      )}
    </Flex>
  );

  if (embedded) {
    return (
      <Spin spinning={spinning}>
        <div className={className}>{content}</div>
      </Spin>
    );
  }

  return (
    <Card bordered={false} className={['strategy-chat-card', className].filter(Boolean).join(' ')}>
      <Spin spinning={spinning}>{content}</Spin>
    </Card>
  );
};

export default StrategyChatCard;
