import type { FillItem } from '@api/types';
import { Card, List, Tag, Typography } from 'antd';
import dayjs from 'dayjs';

interface Props {
  fills?: FillItem[];
  loading?: boolean;
}

const TradeActivityList = ({ fills, loading }: Props) => {
  const data = fills ? [...fills].sort((a, b) => Number(b.timestamp) - Number(a.timestamp)).slice(0, 50) : [];

  return (
    <Card title="执行明细" bordered={false} loading={loading}>
      <List
        dataSource={data}
        split={false}
        renderItem={(item) => {
          const sideLabel = item.side === 'buy' ? '买入' : '卖出';
          const sideColor = item.side === 'buy' ? 'green' : 'red';
          const price = Number(item.price);
          const size = Number(item.size);
          const notional = price * size;
          const fee = Number(item.fee ?? 0);

          return (
            <List.Item>
              <div className="trade-activity-item">
                <div className="trade-activity-header">
                  <Tag color={sideColor}>{sideLabel}</Tag>
                  <Typography.Text strong>{item.symbol}</Typography.Text>
                  <Typography.Text type="secondary">
                    {dayjs(Number(item.timestamp)).format('MM-DD HH:mm:ss')}
                  </Typography.Text>
                </div>
                <div className="trade-activity-body">
                  <div>
                    <Typography.Text type="secondary">成交价</Typography.Text>
                    <Typography.Text>{price.toLocaleString()}</Typography.Text>
                  </div>
                  <div>
                    <Typography.Text type="secondary">数量</Typography.Text>
                    <Typography.Text>{size.toLocaleString()}</Typography.Text>
                  </div>
                  <div>
                    <Typography.Text type="secondary">名义金额</Typography.Text>
                    <Typography.Text>{notional.toLocaleString(undefined, { maximumFractionDigits: 2 })}</Typography.Text>
                  </div>
                  <div>
                    <Typography.Text type="secondary">手续费</Typography.Text>
                    <Typography.Text>{fee.toLocaleString(undefined, { maximumFractionDigits: 4 })}</Typography.Text>
                  </div>
                </div>
              </div>
            </List.Item>
          );
        }}
      />
    </Card>
  );
};

export default TradeActivityList;
