const DEFAULT_BRAND_NAME = 'NovaTrade AI';
const DEFAULT_BRAND_TAGLINE = '智能量化交易平台';

const pickValue = (value: string | undefined, fallback: string) => {
  const trimmed = value?.trim();
  return trimmed?.length ? trimmed : fallback;
};

export const BRAND_NAME = pickValue(import.meta.env.VITE_APP_BRAND_NAME, DEFAULT_BRAND_NAME);
export const BRAND_TAGLINE = pickValue(
  import.meta.env.VITE_APP_BRAND_TAGLINE,
  DEFAULT_BRAND_TAGLINE
);
export const BRAND_CONSOLE_TITLE = BRAND_NAME;
