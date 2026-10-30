/** 主题外观与强调色：写到 <html> 的 data-*，由 CSS 变量接管。 */
export type ThemeAppearance = 'system' | 'light' | 'dark';
export type AccentId = 'red' | 'terracotta';

export const ACCENT_COLORS: Record<AccentId, string> = {
  red: '#e63c3c',
  terracotta: '#d97757',
};

export const ACCENT_LABELS: Record<AccentId, string> = {
  red: '网易云红',
  terracotta: '陶土橙',
};

export const THEME_LABELS: Record<ThemeAppearance, string> = {
  system: '跟随系统',
  light: '浅色',
  dark: '深色',
};

let media: MediaQueryList | null = null;
let mediaHandler: ((e: MediaQueryListEvent) => void) | null = null;

function resolvedTheme(appearance: ThemeAppearance): 'light' | 'dark' {
  if (appearance === 'light' || appearance === 'dark') return appearance;
  if (typeof window !== 'undefined' && window.matchMedia) {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  return 'light';
}

function bindSystemListener(appearance: ThemeAppearance) {
  if (typeof window === 'undefined' || !window.matchMedia) return;
  if (media && mediaHandler) {
    media.removeEventListener('change', mediaHandler);
    media = null;
    mediaHandler = null;
  }
  if (appearance !== 'system') return;
  media = window.matchMedia('(prefers-color-scheme: dark)');
  mediaHandler = () => {
    const root = document.documentElement;
    root.dataset.theme = resolvedTheme('system');
  };
  media.addEventListener('change', mediaHandler);
}

/** 根据设置应用到 documentElement；可在 settings 加载前用默认值调用。 */
export function applyTheme(opts: {
  themeAppearance?: ThemeAppearance | string | null;
  accent?: AccentId | string | null;
}) {
  const appearance = normalizeAppearance(opts.themeAppearance);
  const accent = normalizeAccent(opts.accent);
  const root = document.documentElement;
  root.dataset.themeAppearance = appearance;
  root.dataset.theme = resolvedTheme(appearance);
  root.dataset.accent = accent;
  root.style.setProperty('--accent', ACCENT_COLORS[accent]);
  bindSystemListener(appearance);
}

export function normalizeAppearance(v: unknown): ThemeAppearance {
  return v === 'light' || v === 'dark' || v === 'system' ? v : 'system';
}

export function normalizeAccent(v: unknown): AccentId {
  return v === 'terracotta' || v === 'red' ? v : 'red';
}
