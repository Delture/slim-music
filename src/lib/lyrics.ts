/** 歌词缓存：内存 +（经 Rust）磁盘。网易云按 ncm id；本地按 track id / 路径。 */
import { api } from './api';
import type { LyricPayload, Track } from './types';

const mem = new Map<string, LyricPayload>();

function ncmKey(id: number) {
  return `ncm:${id}`;
}

export function peekLyric(key: string): LyricPayload | null {
  return mem.get(key) ?? null;
}

export function peekLyricForTrack(t: Track): LyricPayload | null {
  return mem.get(t.id) ?? null;
}

/** 网易云：先内存，再后端（磁盘缓存）。 */
export async function loadLyricCached(ncmId: number): Promise<LyricPayload> {
  const key = ncmKey(ncmId);
  const hit = mem.get(key);
  if (hit) return hit;
  const payload = await api.lyric(ncmId);
  mem.set(key, payload);
  return payload;
}

/** 本地：同名 .lrc sidecar；内存 + 后端磁盘缓存。 */
export async function loadLocalLyricCached(trackId: string, path: string): Promise<LyricPayload> {
  const hit = mem.get(trackId);
  if (hit) return hit;
  const payload = await api.localLyric(path);
  mem.set(trackId, payload);
  return payload;
}

export function clearLyricMemory() {
  mem.clear();
}
