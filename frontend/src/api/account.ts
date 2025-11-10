import client from './client';
import type {
  ApiResponse,
  BalanceItem,
  FillItem,
  InitialEquityRecord,
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
  const { data } = await client.get<ApiResponse<RawBalanceItem[]>>('/account/balances');
  return data.data.map<BalanceItem>((item) => ({
    asset: item.asset,
    available: item.available,
    locked: item.locked,
    valuationUSDT: item.valuationUSDT ?? item.valuation_usdt
  }));
};

export const fetchPositions = async () => {
  const { data } = await client.get<ApiResponse<PositionItem[]>>('/account/positions');
  return data.data;
};

export const fetchPositionHistory = async (params: { symbol?: string; limit?: number } = {}) => {
  const { data } = await client.get<ApiResponse<PositionHistoryItem[]>>('/account/positions/history', {
    params
  });
  return data.data;
};

export const fetchFills = async (params: { symbol?: string; limit?: number }) => {
  const { data } = await client.get<ApiResponse<FillItem[]>>('/account/fills', {
    params
  });
  return data.data;
};

export const fetchInitialEquity = async () => {
  const { data } = await client.get<ApiResponse<InitialEquityRecord | null>>(
    '/account/initial-equity'
  );
  return data.data ?? null;
};

export const setInitialEquity = async (payload: { amount: number }) => {
  const { data } = await client.post<ApiResponse<InitialEquityRecord | null>>(
    '/account/initial-equity',
    payload
  );
  return data.data ?? null;
};
