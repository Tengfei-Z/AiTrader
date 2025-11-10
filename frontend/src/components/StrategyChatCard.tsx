import type { StrategyMessage } from '@api/types';
import { Card, Empty, Flex, List, Typography, Button, Spin } from 'antd';
import dayjs from 'dayjs';

interface Props {
  messages?: StrategyMessage[];
  loading?: boolean;
  className?: string;
  onRefresh?: () => void;
  onStart?: () => void;
  starting?: boolean;
  embedded?: boolean;
}

const StrategyChatCard = ({
  messages,
  loading,
  className,
  onRefresh,
  onStart,
  starting,
  embedded
}: Props) => {
  const orderedMessages = (messages ?? [])
    .slice()
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
  const spinning = Boolean(loading || starting);

  const content = (
    <Flex vertical gap={16} className="strategy-chat-panel">
      <Flex justify="space-between" align="center" className="strategy-chat-panel__toolbar">
        <Typography.Title level={5} className="strategy-chat-panel__title">
          策略对话
        </Typography.Title>
        <Flex align="center" gap={8}>
          {onStart && (
            <Button type="primary" size="small" onClick={onStart} loading={starting}>
              启动策略
            </Button>
          )}
          {onRefresh && (
            <Button type="link" size="small" onClick={onRefresh}>
              刷新
            </Button>
          )}
        </Flex>
      </Flex>
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
                    ellipsis={{
                      rows: 3,
                      expandable: 'collapsible',
                      symbol: (expanded) => (expanded ? '收起' : '展开')
                    }}
                  >
                    {item.summary}
                  </Typography.Paragraph>
                </Flex>
              </List.Item>
            )}
          />
        </div>
      )}
      <div className="strategy-chat-actions">
        <Typography.Text type="secondary">
          对话由大模型生成，建议在执行前再次确认关键指令。
        </Typography.Text>
      </div>
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
