import { create } from 'zustand';

interface SymbolState {
  symbol: string;
  setSymbol: (symbol: string) => void;
}

export const useSymbolStore = create<SymbolState>((set) => ({
  symbol: 'BTC-USDT-SWAP',
  setSymbol: (symbol) => set({ symbol })
}));
