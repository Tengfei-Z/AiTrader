import { Layout, Typography } from 'antd';
import AiConsolePage from '@pages/Console';
import { BRAND_NAME, BRAND_TAGLINE } from '@utils/branding';
import RocketBadge from '@components/RocketBadge';

const { Header, Content } = Layout;

const App = () => {
  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Header className="header">
        <div className="header-brand">
          <RocketBadge />
          <div className="header-identity">
            <span className="header-logo">{BRAND_NAME}</span>
            <Typography.Text className="header-subtitle">{BRAND_TAGLINE}</Typography.Text>
          </div>
        </div>
      </Header>
      <Content className="content">
        <AiConsolePage />
      </Content>
    </Layout>
  );
};

export default App;
