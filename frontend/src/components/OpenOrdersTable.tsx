import { cancelOrder } from '@api/orders';
import type { OrderItem } from '@api/types';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Button, Card, Table, message } from 'antd';
import dayjs from 'dayjs';

interface Props {
  orders?: OrderItem[];
  loading?: boolean;
}

const OpenOrdersTable = ({ orders, loading }: Props) => {
  const queryClient = useQueryClient();

  const { mutate, isPending } = useMutation({
    mutationFn: cancelOrder,
    onSuccess: () => {
      message.success('撤单成功');
      queryClient.invalidateQueries({ queryKey: ['open-orders'] });
      queryClient.invalidateQueries({ queryKey: ['balances'] });
    },
    onError: (err: any) => {
      const detail = err?.response?.data?.error ?? err.message;
      message.error(`撤单失败：${detail}`);
    }
  });

  const columns = [
    {
      title: '时间',
      dataIndex: 'createdAt',
      key: 'createdAt',
      render: (value: string) => dayjs(Number(value)).format('MM-DD HH:mm:ss')
    },
    { title: '方向', dataIndex: 'side', key: 'side' },
    { title: '类型', dataIndex: 'type', key: 'type' },
    { title: '价格', dataIndex: 'price', key: 'price' },
    { title: '数量', dataIndex: 'size', key: 'size' },
    { title: '已成交', dataIndex: 'filledSize', key: 'filledSize' },
    {
      title: '操作',
      key: 'action',
      render: (_: unknown, record: OrderItem) => (
        <Button size="small" danger loading={isPending} onClick={() => mutate(record.orderId)}>
          撤单
        </Button>
      )
    }
  ];

  return (
    <Card title="当前委托" bordered={false} loading={loading}>
      <Table rowKey="orderId" pagination={false} dataSource={orders} columns={columns} size="small" />
    </Card>
  );
};

export default OpenOrdersTable;
