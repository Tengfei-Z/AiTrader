import { Col, Flex, Row, Select } from 'antd';
import { useMemo, useState } from 'react';
import BalancesTable from '@components/BalancesTable';
import FillsTable from '@components/FillsTable';
import OrderHistoryTable from '@components/OrderHistoryTable';
import { useBalances } from '@hooks/useBalances';
import { useFills } from '@hooks/useFills';
import { useOrderHistory } from '@hooks/useOrderHistory';
import { useSymbolStore } from '@store/useSymbolStore';

const symbols = ['BTC-USDT', 'ETH-USDT', 'SOL-USDT'];

const AccountPage = () => {
  const globalSymbol = useSymbolStore((state) => state.symbol);
  const [symbol, setSymbol] = useState<string | undefined>(globalSymbol);

  const { data: balances, isLoading: balancesLoading } = useBalances();
  const fillsParams = useMemo(() => ({ symbol, limit: 50 }), [symbol]);
  const historyParams = useMemo(() => ({ symbol, limit: 50 }), [symbol]);
  const { data: fills, isLoading: fillsLoading } = useFills(fillsParams.symbol, fillsParams.limit);
  const { data: orderHistory, isLoading: historyLoading } = useOrderHistory(historyParams, true);

  const symbolOptions = useMemo(
    () =>
      [{ value: '', label: '全部' }, ...symbols.map((s) => ({ value: s, label: s }))] as { value: string; label: string }[],
    []
  );

  return (
    <Flex vertical gap={24}>
      <BalancesTable balances={balances} loading={balancesLoading} />
      <Select
        placeholder="选择交易对过滤"
        allowClear
        value={symbol ?? ''}
        options={symbolOptions}
        onChange={(value) => setSymbol(value || undefined)}
        style={{ width: 240 }}
      />
      <Row gutter={24}>
        <Col span={12}>
          <OrderHistoryTable orders={orderHistory} loading={historyLoading} />
        </Col>
        <Col span={12}>
          <FillsTable fills={fills} loading={fillsLoading} />
        </Col>
      </Row>
    </Flex>
  );
};

export default AccountPage;
