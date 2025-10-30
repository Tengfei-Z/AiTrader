import { placeOrder } from '@api/orders';
import type { PlaceOrderPayload } from '@api/types';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Button, Card, Divider, Form, InputNumber, Radio, Select, message } from 'antd';
import { useState } from 'react';

interface Props {
  symbol: string;
}

const OrderForm = ({ symbol }: Props) => {
  const queryClient = useQueryClient();
  const [form] = Form.useForm<PlaceOrderPayload>();
  const [side, setSide] = useState<'buy' | 'sell'>('buy');
  const [type, setType] = useState<'limit' | 'market'>('limit');

  const mutation = useMutation({
    mutationFn: placeOrder,
    onSuccess: (data) => {
      message.success(`下单成功，订单号：${data.orderId}`);
      queryClient.invalidateQueries({ queryKey: ['balances'] });
      queryClient.invalidateQueries({ queryKey: ['open-orders'] });
      queryClient.invalidateQueries({ queryKey: ['fills'] });
      form.resetFields(['price', 'size']);
    },
    onError: (err: any) => {
      const detail = err?.response?.data?.error ?? err.message;
      message.error(`下单失败：${detail}`);
    }
  });

  const onFinish = (values: PlaceOrderPayload) => {
    mutation.mutate({ ...values, symbol });
  };

  return (
    <Card
      title="下单"
      bordered={false}
      extra={<span style={{ color: side === 'buy' ? '#16a34a' : '#dc2626' }}>{side === 'buy' ? '买入' : '卖出'}</span>}
    >
      <Form
        layout="vertical"
        form={form}
        initialValues={{ symbol, side, type, timeInForce: 'gtc' }}
        onValuesChange={(changed) => {
          if (changed.side) setSide(changed.side);
          if (changed.type) setType(changed.type);
        }}
        onFinish={onFinish}
      >
        <Form.Item name="side" label="方向">
          <Radio.Group>
            <Radio.Button value="buy">买入</Radio.Button>
            <Radio.Button value="sell">卖出</Radio.Button>
          </Radio.Group>
        </Form.Item>

        <Form.Item name="type" label="订单类型">
          <Radio.Group>
            <Radio.Button value="limit">限价</Radio.Button>
            <Radio.Button value="market">市价</Radio.Button>
          </Radio.Group>
        </Form.Item>

        {type === 'limit' && (
          <Form.Item
            name="price"
            label="价格 (USDT)"
            rules={[{ required: true, message: '请输入价格' }]}
          >
            <InputNumber min={0} precision={2} style={{ width: '100%' }} placeholder="输入价格" />
          </Form.Item>
        )}

        <Form.Item
          name="size"
          label="数量 (BTC)"
          rules={[{ required: true, message: '请输入数量' }]}
        >
          <InputNumber min={0} precision={6} style={{ width: '100%' }} placeholder="输入数量" />
        </Form.Item>

        <Form.Item name="timeInForce" label="成交策略">
          <Select
            options={[
              { value: 'gtc', label: 'GTC - 一直有效' },
              { value: 'ioc', label: 'IOC - 立即成交剩余撤销' },
              { value: 'fok', label: 'FOK - 全部成交否则撤销' }
            ]}
          />
        </Form.Item>

        <Divider />

        <Button type="primary" htmlType="submit" loading={mutation.isPending} block>
          提交订单
        </Button>
      </Form>
    </Card>
  );
};

export default OrderForm;
