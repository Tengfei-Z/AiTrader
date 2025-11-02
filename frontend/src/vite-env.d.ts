/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_OKX_SIMULATED?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
