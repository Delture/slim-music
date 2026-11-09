/** 桌面端播放页沉浸式标题栏。
 *  - macOS：TitleBarStyle overlay（保留红绿灯）
 *  - Windows：进出播放页切换 decorations，自定义拖拽区与窗口按钮
 *  - Android / 浏览器：不操作
 */
import { isMobile } from './native';

type Platform = 'mac' | 'win';
type MacStyle = 'visible' | 'overlay';

let platform: Platform | null | undefined;
let macApplied: MacStyle | null = null;
let winImmersive = false;
let pending: Promise<void> | null = null;

function detectPlatform(): Platform | null {
  if (platform !== undefined) return platform;
  if (isMobile) {
    platform = null;
    return null;
  }
  if (typeof navigator === 'undefined') {
    platform = null;
    return null;
  }
  const ua = navigator.userAgent || '';
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown; __TAURI__?: unknown };
  if (!(w.__TAURI_INTERNALS__ || w.__TAURI__)) {
    platform = null;
    return null;
  }
  if (/Macintosh|Mac OS X/i.test(ua)) {
    platform = 'mac';
    return 'mac';
  }
  if (/Windows/i.test(ua)) {
    platform = 'win';
    return 'win';
  }
  platform = null;
  return null;
}

function setMacOverlayClass(on: boolean) {
  if (typeof document === 'undefined') return;
  document.documentElement.classList.toggle('sm-np-overlay', on);
}

function setWinImmersiveClass(on: boolean) {
  if (typeof document === 'undefined') return;
  document.documentElement.classList.toggle('sm-np-win', on);
}

async function applyMac(style: MacStyle): Promise<void> {
  if (macApplied === style) {
    setMacOverlayClass(style === 'overlay');
    return;
  }
  setMacOverlayClass(style === 'overlay');
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().setTitleBarStyle(style);
    macApplied = style;
  } catch (e) {
    console.warn('[titlebar] setTitleBarStyle failed', e);
    if (style === 'overlay') setMacOverlayClass(false);
  }
}

async function applyWin(immersive: boolean): Promise<void> {
  if (winImmersive === immersive) {
    setWinImmersiveClass(immersive);
    return;
  }
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const win = getCurrentWindow();
  if (immersive) {
    // 先挂自定义顶栏，再去系统标题栏，避免短暂无按钮
    setWinImmersiveClass(true);
    try {
      await win.setDecorations(false);
      winImmersive = true;
    } catch (e) {
      console.warn('[titlebar] setDecorations(false) failed', e);
      setWinImmersiveClass(false);
      winImmersive = false;
    }
    return;
  }
  // 离开：先恢复系统标题栏，成功后再收自定义控件
  try {
    await win.setDecorations(true);
    winImmersive = false;
    setWinImmersiveClass(false);
  } catch (e) {
    console.warn('[titlebar] setDecorations(true) failed', e);
    // 仍无系统栏时保留自定义按钮，避免窗口失控
    setWinImmersiveClass(true);
  }
}

async function apply(immersive: boolean): Promise<void> {
  const p = detectPlatform();
  if (p === 'mac') {
    await applyMac(immersive ? 'overlay' : 'visible');
    return;
  }
  if (p === 'win') {
    await applyWin(immersive);
  }
}

/** 串行化切换，避免进出播放页时竞态。 */
export function setNowPlayingTitlebar(immersive: boolean): Promise<void> {
  const run = (pending ?? Promise.resolve()).then(() => apply(immersive));
  pending = run.then(
    () => undefined,
    () => undefined,
  );
  return run;
}

export function isWinNowPlayingChrome(): boolean {
  return detectPlatform() === 'win';
}

async function withWindow(fn: (w: Awaited<ReturnType<typeof import('@tauri-apps/api/window').getCurrentWindow>>) => Promise<void>) {
  if (detectPlatform() !== 'win') return;
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await fn(getCurrentWindow());
  } catch (e) {
    console.warn('[titlebar] window control failed', e);
  }
}

export function winMinimize(): Promise<void> {
  return withWindow((w) => w.minimize());
}

export function winToggleMaximize(): Promise<void> {
  return withWindow((w) => w.toggleMaximize());
}

export function winClose(): Promise<void> {
  return withWindow((w) => w.close());
}
