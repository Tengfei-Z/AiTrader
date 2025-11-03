import type { StrategyMessage } from '@api/types';
import { Card, Empty, Flex, List, Tag, Typography, Button, Spin } from 'antd';
import dayjs from 'dayjs';

interface Props {
  messages?: StrategyMessage[];
  loading?: boolean;
  className?: string;
  onRefresh?: () => void;
  embedded?: boolean;
}

const roleMap: Record<StrategyMessage['role'], { label: string; color: string }> = {
  assistant: { label: '策略引擎', color: 'geekblue' },
  user: { label: '人工指令', color: 'green' },
  system: { label: '系统提示', color: 'gold' }
};

const StrategyChatCard = ({ messages, loading, className, onRefresh, embedded }: Props) => {
  const sortedMessages = (messages ?? []).slice().sort((a, b) => a.createdAt.localeCompare(b.createdAt));

  const content = (
    <Flex vertical gap={16} className="strategy-chat-panel">
      <Flex justify="space-between" align="center" className="strategy-chat-panel__toolbar">
        <Typography.Title level={5} className="strategy-chat-panel__title">
          策略对话
        </Typography.Title>
        {onRefresh && (
          <Button type="link" size="small" onClick={onRefresh}>
            刷新
          </Button>
        )}
      </Flex>
      {sortedMessages.length === 0 ? (
        <Empty description="暂无对话记录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      ) : (
        <div className="strategy-chat-history">
          <List
            dataSource={sortedMessages}
            renderItem={(item) => {
              const role = roleMap[item.role] ?? roleMap.system;
              return (
                <List.Item className="strategy-chat-message">
                  <Flex vertical gap={6}>
                    <Flex align="center" gap={12} className="strategy-chat-message__meta">
                      <Tag color={role.color}>{role.label}</Tag>
                      <Typography.Text type="secondary">
                        {dayjs(item.createdAt).format('MM-DD HH:mm')}
                      </Typography.Text>
                      {item.summary && (
                        <Typography.Text strong className="strategy-chat-message__summary">
                          {item.summary}
                        </Typography.Text>
                      )}
                    </Flex>
                    <Typography.Paragraph
                      className="strategy-chat-message__content"
                      ellipsis={{ rows: 2, expandable: true, symbol: '展开' }}
                    >
                      {item.content}
                    </Typography.Paragraph>
                    {item.tags && item.tags.length > 0 && (
                      <Flex gap={8} wrap="wrap">
                        {item.tags.map((tag) => (
                          <Tag key={tag} color="default">
                            {tag}
                          </Tag>
                        ))}
                      </Flex>
                    )}
                  </Flex>
                </List.Item>
              );
            }}
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
      <Spin spinning={loading}>
        <div className={className}>{content}</div>
      </Spin>
    );
  }

  return (
    <Card
      bordered={false}
      loading={loading}
      className={['strategy-chat-card', className].filter(Boolean).join(' ')}
    >
      {content}
    </Card>
  );
};

export default StrategyChatCard;
