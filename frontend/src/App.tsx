import { Layout, Typography } from 'antd';
import AiConsolePage from '@pages/Console';

const { Header, Content } = Layout;

const App = () => {
  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Header className="header">
        <div className="header-brand">
          <span className="header-logo">AiTrader</span>
        </div>
      </Header>
      <Content className="content">
        <AiConsolePage />
      </Content>
    </Layout>
  );
};

export default App;
