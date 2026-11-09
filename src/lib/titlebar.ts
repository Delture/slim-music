/** macOS 沉浸式标题栏：仅桌面 Mac 在 Now Playing 时切 overlay，离开后恢复。 */
import { isMobile } from './native';

type Style = 'visible' | 'overlay';

let applied: Style | null = null;
let pending: Promise<void> | null = null;

function isDesktopMac(): boolean {
  if (isMobile) return false;
  if (typeof navigator === 'undefined') return false;
  const ua = navigator.userAgent || '';
  // Tauri WebView on macOS reports Macintosh; skip plain browser / Android / Windows.
  if (!/Macintosh|Mac OS X/i.test(ua)) return false;
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown; __TAURI__?: unknown };
  return !!(w.__TAURI_INTERNALS__ || w.__TAURI__);
}

function setOverlayClass(on: boolean) {
  if (typeof document === 'undefined') return;
  document.documentElement.classList.toggle('sm-np-overlay', on);
}

async function apply(style: Style): Promise<void> {
  if (!isDesktopMac()) return;
  if (applied === style) {
    setOverlayClass(style === 'overlay');
    return;
  }
  setOverlayClass(style === 'overlay');
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().setTitleBarStyle(style);
    applied = style;
  } catch (e) {
    console.warn('[titlebar] setTitleBarStyle failed', e);
    if (style === 'overlay') setOverlayClass(false);
  }
}

/** 串行化切换，避免进出播放页时竞态。 */
export function setNowPlayingTitlebar(immersive: boolean): Promise<void> {
  const style: Style = immersive ? 'overlay' : 'visible';
  const run = (pending ?? Promise.resolve()).then(() => apply(style));
  pending = run.then(
    () => undefined,
    () => undefined,
  );
  return run;
}
