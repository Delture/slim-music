import { reactive } from 'vue';
import { api } from './api';
import type { Profile, Settings } from './types';
import { setVolume } from './player';
import { refreshLocalPlaylists } from './localPlaylists';

export const session = reactive({
  profile: null as Profile | null,
  settings: { quality: 'exhigh', localFolders: [], volume: 0.8, cacheEnabled: true, audioCacheMaxMb: 2048 } as Settings,
  /** 已红心的网易云歌曲 id */
  likedIds: new Set<number>(),
  /** 全屏播放页是否打开 */
  nowPlaying: false,
});

export async function initSession() {
  session.settings = await api.getSettings();
  setVolume(session.settings.volume);
  await refreshProfile();
  await refreshLocalPlaylists().catch(() => {});
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
  // 触发响应式：替换 Set
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
  await api.saveSettings(session.settings);
}

export function openNowPlaying() {
  session.nowPlaying = true;
}
export function closeNowPlaying() {
  session.nowPlaying = false;
}
