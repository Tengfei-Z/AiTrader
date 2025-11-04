import { Select } from 'antd';
import { useSymbolStore } from '@store/useSymbolStore';

const symbols = ['BTC-USDT-SWAP'];

const SymbolSelector = () => {
  const symbol = useSymbolStore((state) => state.symbol);
  const setSymbol = useSymbolStore((state) => state.setSymbol);

  return (
    <Select
      value={symbol}
      style={{ width: 180 }}
      onChange={setSymbol}
      options={symbols.map((item) => ({ value: item, label: item }))}
    />
  );
};

export default SymbolSelector;
