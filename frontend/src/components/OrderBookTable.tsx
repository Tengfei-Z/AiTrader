import { Card, Col, Row, Table } from 'antd';
import type { OrderBook } from '@api/types';

interface Props {
  orderBook?: OrderBook;
  loading?: boolean;
}

const columns = (type: 'bids' | 'asks') => [
  {
    title: '价格',
    dataIndex: 0,
    key: 'price',
    render: (value: string) => (
      <span style={{ color: type === 'bids' ? '#16a34a' : '#dc2626' }}>{Number(value).toLocaleString()}</span>
    )
  },
  {
    title: '数量',
    dataIndex: 1,
    key: 'size',
    render: (value: string) => Number(value).toLocaleString()
  }
];

const OrderBookTable = ({ orderBook, loading }: Props) => (
  <Card bordered={false} loading={loading} title="盘口深度">
    <Row gutter={16}>
      <Col span={12}>
        <Table
          size="small"
          pagination={false}
          columns={columns('bids')}
          dataSource={orderBook?.bids?.slice(0, 10).map((row, idx) => ({ key: `bid-${idx}`, ...row }))}
        />
      </Col>
      <Col span={12}>
        <Table
          size="small"
          pagination={false}
          columns={columns('asks')}
          dataSource={orderBook?.asks?.slice(0, 10).map((row, idx) => ({ key: `ask-${idx}`, ...row }))}
        />
      </Col>
    </Row>
  </Card>
);

export default OrderBookTable;
