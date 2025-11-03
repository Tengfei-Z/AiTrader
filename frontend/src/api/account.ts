import client from './client';
import type {
  ApiResponse,
  BalanceItem,
  FillItem,
  OrderItem,
  PositionHistoryItem,
  PositionItem
} from './types';

interface RawBalanceItem {
  asset: string;
  available: string;
  locked: string;
  valuation_usdt?: string;
  valuationUSDT?: string;
}

export const fetchBalances = async () => {
  const simulatedFlag = import.meta.env.VITE_OKX_SIMULATED;
  const { data } = await client.get<ApiResponse<RawBalanceItem[]>>('/account/balances', {
    params:
      simulatedFlag === 'false'
        ? undefined
        : { simulated: true }
  });
  return data.data.map<BalanceItem>((item) => ({
    asset: item.asset,
    available: item.available,
    locked: item.locked,
    valuationUSDT: item.valuationUSDT ?? item.valuation_usdt
  }));
};

export const fetchPositions = async () => {
  const simulatedFlag = import.meta.env.VITE_OKX_SIMULATED;
  const { data } = await client.get<ApiResponse<PositionItem[]>>('/account/positions', {
    params:
      simulatedFlag === 'false'
        ? undefined
        : { simulated: true }
  });
  return data.data;
};

export const fetchPositionHistory = async (params: { symbol?: string; limit?: number; simulated?: boolean } = {}) => {
  const simulatedFlag = import.meta.env.VITE_OKX_SIMULATED;
  const { data } = await client.get<ApiResponse<PositionHistoryItem[]>>('/account/positions/history', {
    params: {
      ...params,
      simulated: simulatedFlag === 'false' ? params.simulated : true
    }
  });
  return data.data;
};

export const fetchOpenOrders = async (symbol?: string) => {
  const { data } = await client.get<ApiResponse<OrderItem[]>>('/account/orders/open', {
    params: { symbol }
  });
  return data.data;
};

export const fetchFills = async (params: { symbol?: string; limit?: number; simulated?: boolean }) => {
  const simulatedFlag = import.meta.env.VITE_OKX_SIMULATED;
  const { data } = await client.get<ApiResponse<FillItem[]>>('/account/fills', {
    params: {
      ...params,
      simulated: simulatedFlag === 'false' ? params.simulated : true
    }
  });
  return data.data;
};
