/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_APP_BRAND_NAME?: string;
  readonly VITE_APP_BRAND_TAGLINE?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
