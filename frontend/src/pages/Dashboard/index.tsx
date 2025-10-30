import { Flex } from 'antd';
import MultiTickerCard from '@components/MultiTickerCard';

const DashboardPage = () => {
  return (
    <Flex vertical gap={24}>
      <MultiTickerCard />
    </Flex>
  );
};

export default DashboardPage;
