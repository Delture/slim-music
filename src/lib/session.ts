import { reactive } from 'vue';
import { api } from './api';
import type { AccentId, Profile, Settings, ThemeAppearance } from './types';
import { applyPlaybackPrefs, restorePlaybackSession, setVolume } from './player';
import { refreshLocalPlaylists } from './localPlaylists';
import { applyTheme, normalizeAccent, normalizeAppearance } from './theme';

const defaultSettings = (): Settings => ({
  quality: 'exhigh',
  localFolders: [],
  volume: 0.8,
  shuffle: false,
  repeat: 'off',
  cacheEnabled: true,
  audioCacheMaxMb: 2048,
  accent: 'red',
  themeAppearance: 'system',
});

export const session = reactive({
  profile: null as Profile | null,
  settings: defaultSettings(),
  /** 已红心的网易云歌曲 id */
  likedIds: new Set<number>(),
  /** 全屏播放页是否打开 */
  nowPlaying: false,
});

let persistTimer: number | undefined;

export async function initSession() {
  const loaded = await api.getSettings();
  session.settings = {
    ...defaultSettings(),
    ...loaded,
    volume: clampVol(loaded.volume),
    shuffle: !!loaded.shuffle,
    repeat: normalizeRepeat(loaded.repeat),
    accent: normalizeAccent(loaded.accent) as AccentId,
    themeAppearance: normalizeAppearance(loaded.themeAppearance) as ThemeAppearance,
  };
  applyTheme({
    accent: session.settings.accent,
    themeAppearance: session.settings.themeAppearance,
  });
  setVolume(session.settings.volume, { persist: false });
  applyPlaybackPrefs(session.settings.shuffle, session.settings.repeat);
  await restorePlaybackSession().catch(console.warn);
  await refreshProfile();
  await refreshLocalPlaylists().catch(() => {});
}

function clampVol(v: unknown): number {
  const n = typeof v === 'number' ? v : Number(v);
  if (!Number.isFinite(n)) return 0.8;
  return Math.min(1, Math.max(0, n));
}

function normalizeRepeat(r: unknown): Settings['repeat'] {
  return r === 'all' || r === 'one' || r === 'off' ? r : 'off';
}

export async function refreshProfile() {
  try {
    session.profile = await api.profile();
    if (session.profile) await refreshLikedIds();
    else session.likedIds = new Set();
  } catch {
    session.profile = null;
    session.likedIds = new Set();
  }
}

export async function refreshLikedIds() {
  try {
    const ids = await api.likedIds();
    session.likedIds = new Set(ids);
  } catch {
    /* 未登录等 */
  }
}

export function isLiked(ncmId: number | null | undefined): boolean {
  return ncmId != null && session.likedIds.has(ncmId);
}

/** 乐观更新红心；失败时回滚 */
export async function toggleLike(ncmId: number): Promise<boolean> {
  const next = !session.likedIds.has(ncmId);
  const snap = new Set(session.likedIds);
  if (next) session.likedIds.add(ncmId);
  else session.likedIds.delete(ncmId);
  session.likedIds = new Set(session.likedIds);
  try {
    await api.likeTrack(ncmId, next);
    return next;
  } catch (e) {
    session.likedIds = snap;
    throw e;
  }
}

export async function saveSettings() {
  applyTheme({
    accent: session.settings.accent,
    themeAppearance: session.settings.themeAppearance,
  });
  await api.saveSettings(session.settings);
}

/** 设置外观 / 强调色并立即持久化 */
export async function setAppearance(partial: {
  accent?: AccentId;
  themeAppearance?: ThemeAppearance;
}) {
  if (partial.accent) session.settings.accent = partial.accent;
  if (partial.themeAppearance) session.settings.themeAppearance = partial.themeAppearance;
  await saveSettings();
}

/** 把当前播放器音量/随机/循环写回 settings.json（防抖） */
export function persistPlaybackSettings(delayMs = 250) {
  session.settings.volume = clampVol(session.settings.volume);
  if (persistTimer) window.clearTimeout(persistTimer);
  persistTimer = window.setTimeout(() => {
    void saveSettings().catch(() => {});
  }, delayMs);
}

export function openNowPlaying() {
  session.nowPlaying = true;
}
export function closeNowPlaying() {
  session.nowPlaying = false;
}
