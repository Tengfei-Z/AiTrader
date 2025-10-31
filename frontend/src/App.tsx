import { Layout, Menu } from 'antd';
import { MenuProps } from 'antd';
import { useMemo, useState } from 'react';
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom';
import DashboardPage from '@pages/Dashboard';
import TradePage from '@pages/Trade';
import AccountPage from '@pages/Account';

const menuItems: MenuProps['items'] = [
  { key: '/dashboard', label: '行情概览' },
  { key: '/trade', label: '交易下单' },
  { key: '/account', label: '账户中心' }
];

const { Header, Sider, Content } = Layout;

const App = () => {
  const location = useLocation();
  const navigate = useNavigate();
  const [collapsed, setCollapsed] = useState(false);

  const selectedKeys = useMemo(() => {
    const pathname = location.pathname;
    const matched = menuItems?.find((item) => item?.key && pathname.startsWith(item.key));
    return matched?.key ? [matched.key.toString()] : ['/dashboard'];
  }, [location.pathname]);

  const onMenuClick: MenuProps['onClick'] = (info) => {
    navigate(info.key);
  };

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider collapsible collapsed={collapsed} onCollapse={setCollapsed} breakpoint="lg" theme="light">
        <div className="logo">AiTrader</div>
        <Menu theme="light" mode="inline" items={menuItems} selectedKeys={selectedKeys} onClick={onMenuClick} />
      </Sider>
      <Layout>
        <Header className="header">
          <span>AiTrader 控制台</span>
        </Header>
        <Content className="content">
          <Routes>
            <Route path="/" element={<Navigate to="/dashboard" replace />} />
            <Route path="/dashboard" element={<DashboardPage />} />
            <Route path="/trade" element={<TradePage />} />
            <Route path="/account" element={<AccountPage />} />
            <Route path="*" element={<Navigate to="/dashboard" replace />} />
          </Routes>
        </Content>
      </Layout>
    </Layout>
  );
};

export default App;
