import { Layout, Typography } from 'antd';
import AiConsolePage from '@pages/Console';

const { Header, Content } = Layout;

const App = () => {
  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Header className="header">
        <div className="header-brand">
          <span className="header-logo">AiTrader</span>
          <Typography.Text className="header-subtitle">AI 自动交易监控台</Typography.Text>
        </div>
      </Header>
      <Content className="content">
        <AiConsolePage />
      </Content>
    </Layout>
  );
};

export default App;
