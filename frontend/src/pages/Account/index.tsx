import { Flex } from 'antd';
import BalancesTable from '@components/BalancesTable';
import FillsTable from '@components/FillsTable';
import { useBalances } from '@hooks/useBalances';
import { useFills } from '@hooks/useFills';

const AccountPage = () => {
  const { data: balances, isLoading: balancesLoading } = useBalances();
  const { data: fills, isLoading: fillsLoading } = useFills(undefined, 50);

  return (
    <Flex vertical gap={24}>
      <BalancesTable balances={balances} loading={balancesLoading} />
      <FillsTable fills={fills} loading={fillsLoading} />
    </Flex>
  );
};

export default AccountPage;
