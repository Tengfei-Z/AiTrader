import client from './client';
import type {
  ApiResponse,
  BalanceItem,
  BalanceSnapshotItem,
  BalanceSnapshotListPayload,
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

export interface BalanceSnapshotQueryParams {
  asset?: string;
  limit?: number;
  after?: string;
}

const emptySnapshotPayload: BalanceSnapshotListPayload = {
  snapshots: [],
  hasMore: false,
  nextCursor: null
};

export const fetchBalanceSnapshots = async (params: BalanceSnapshotQueryParams = {}) => {
  const { data } = await client.get<ApiResponse<BalanceSnapshotListPayload>>(
    '/account/balances/snapshots',
    {
      params
    }
  );
  return data.data ?? emptySnapshotPayload;
};

export const fetchAllBalanceSnapshots = async (params: BalanceSnapshotQueryParams = {}) => {
  const snapshots: BalanceSnapshotItem[] = [];
  const visitedCursors = new Set<string>();

  const baseParams: BalanceSnapshotQueryParams = { ...params };
  let cursor = baseParams.after;
  delete baseParams.after;

  while (true) {
    const requestParams: BalanceSnapshotQueryParams = cursor
      ? { ...baseParams, after: cursor }
      : { ...baseParams };

    const response = await fetchBalanceSnapshots(requestParams);
    snapshots.push(...response.snapshots);

    if (!response.hasMore || !response.nextCursor) {
      break;
    }
    if (visitedCursors.has(response.nextCursor)) {
      break;
    }

    visitedCursors.add(response.nextCursor);
    cursor = response.nextCursor;
  }

  return snapshots;
};

export const fetchLatestBalanceSnapshot = async (asset?: string) => {
  const { data } = await client.get<ApiResponse<BalanceSnapshotItem | null>>(
    '/account/balances/latest',
    {
      params: asset ? { asset } : undefined
    }
  );
  return data.data;
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
  const { data } = await client.get<ApiResponse<InitialEquityRecord | null>>('/account/initial-equity');
  return data.data ?? null;
};

export const setInitialEquity = async (payload: { amount: number }) => {
  const { data } = await client.post<ApiResponse<InitialEquityRecord | null>>(
    '/account/initial-equity',
    payload
  );
  return data.data ?? null;
};
