/** 网易云歌单曲目的会话内存缓存，切换时立刻显示已打开过的歌单。 */
import { ref } from 'vue';
import type { Track } from './types';

const mem = new Map<number, Track[]>();

/** 设置里清空缓存时递增，歌单页据此丢掉当前曲目并重新拉取 */
export const playlistCacheEpoch = ref(0);

export function getCachedPlaylistTracks(id: number): Track[] | undefined {
  return mem.get(id);
}

export function setCachedPlaylistTracks(id: number, tracks: Track[]) {
  mem.set(id, tracks);
}

export function forgetCachedPlaylistTracks(id: number) {
  mem.delete(id);
}

export function clearPlaylistTrackMemory() {
  mem.clear();
  playlistCacheEpoch.value++;
}
