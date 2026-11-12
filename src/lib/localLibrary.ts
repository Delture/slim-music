// 本机曲库的轻量缓存，供搜索页使用。桌面读索引，Android 走 MediaStore + enrich（含拼音与艺人拆分）。
import { ref, shallowRef } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { api } from './api';
import { isAndroid, nativeListen, queryLocalAudio } from './native';
import { normalizeSearch } from './trackFilter';
import type { LocalLibraryEvent, LocalTrack, Track } from './types';

const MIME: Record<string, string> = {
  mp3: 'audio/mpeg',
  flac: 'audio/flac',
  m4a: 'audio/mp4',
  aac: 'audio/aac',
  wav: 'audio/wav',
  ogg: 'audio/ogg',
  oga: 'audio/ogg',
  opus: 'audio/ogg; codecs="opus"',
  aiff: 'audio/aiff',
  aif: 'audio/aiff',
  ape: 'audio/ape',
  wma: 'audio/x-ms-wma',
  wv: 'audio/wavpack',
};
const ANDROID_DENY = new Set(['ape', 'wma', 'wv']);
const playCache = new Map<string, boolean>();
let probe: HTMLAudioElement | null = null;

function extOf(p: string) {
  const m = /\.([a-z0-9]+)$/i.exec(p);
  return m ? m[1].toLowerCase() : '';
}

function canPlayExt(ext: string): boolean {
  if (!ext) return true;
  if (isAndroid) return !ANDROID_DENY.has(ext);
  let ok = playCache.get(ext);
  if (ok == null) {
    probe ??= document.createElement('audio');
    ok = !!MIME[ext] && probe.canPlayType(MIME[ext]) !== '';
    playCache.set(ext, ok);
  }
  return ok;
}

function filterPlayable(list: LocalTrack[]): LocalTrack[] {
  return list.filter((t) => canPlayExt(extOf(t.path ?? '')));
}

export const localLibraryTracks = shallowRef<LocalTrack[]>([]);
export const localLibraryReady = ref(false);
export const localLibraryError = ref('');

let inflight: Promise<void> | null = null;
let listening = false;
let androidBusy = false;

async function loadAndroid(): Promise<LocalTrack[]> {
  const list = await queryLocalAudio();
  const fileOf = new Map<string, string>();
  const tracks: Track[] = list.map((a) => {
    const id = 'local:' + a.uri;
    fileOf.set(id, a.path || a.uri);
    const tr = a.track ?? 0;
    return {
      id,
      source: 'local' as const,
      title: a.title,
      artists: a.artist ? [a.artist] : [],
      album: a.album,
      durationMs: a.durationMs,
      cover: a.cover || a.albumArt || null,
      path: a.uri,
      ncmId: null,
      trackNo: tr % 1000 || null,
      discNo: Math.floor(tr / 1000) || null,
      year: a.year || null,
      addedMs: a.addedMs || null,
    };
  });
  let enriched: LocalTrack[];
  try {
    enriched = await api.localEnrich(tracks);
  } catch (e) {
    console.warn('localEnrich failed', e);
    enriched = tracks.map((t) => ({
      ...t,
      searchKey: normalizeSearch([t.title, ...t.artists, t.album].join(' ')),
      sortKey: normalizeSearch(t.title),
    }));
  }
  return enriched.filter((t) => canPlayExt(extOf(fileOf.get(t.id) ?? t.path ?? '')));
}

async function loadDesktop(): Promise<LocalTrack[]> {
  try {
    return filterPlayable(await api.localCached());
  } catch {
    return [];
  }
}

/** 拉取本机曲库；并发调用会复用同一次请求。 */
export function refreshLocalLibrary(): Promise<void> {
  if (inflight) return inflight;
  inflight = (async () => {
    try {
      localLibraryError.value = '';
      localLibraryTracks.value = isAndroid ? await loadAndroid() : await loadDesktop();
    } catch (e) {
      localLibraryError.value = String(e);
      // 失败时保留已有结果，避免搜索时本机半边突然空白
      if (!localLibraryTracks.value.length) localLibraryTracks.value = [];
    } finally {
      localLibraryReady.value = true;
    }
  })().finally(() => {
    inflight = null;
  });
  return inflight;
}

async function scanAndroidQuiet() {
  if (androidBusy || !isAndroid) return;
  androidBusy = true;
  try {
    localLibraryTracks.value = await loadAndroid();
    localLibraryError.value = '';
    localLibraryReady.value = true;
  } catch {
    /* 权限等，保留旧结果 */
  } finally {
    androidBusy = false;
  }
}

/** 首次打开搜索页时调用；之后会监听曲库变更。 */
export async function ensureLocalLibrary(): Promise<void> {
  if (!listening) {
    listening = true;
    void listen<LocalLibraryEvent>('local-library-updated', (e) => {
      if (e.payload?.libraryChanged && !isAndroid) void refreshLocalLibrary();
    }).catch(() => {});
    if (isAndroid) {
      void nativeListen((ne) => {
        if (ne.type === 'library') void scanAndroidQuiet();
      }).catch(() => {});
    }
  }
  if (localLibraryReady.value && !inflight) return;
  await refreshLocalLibrary();
}
