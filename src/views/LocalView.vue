<script setup lang="ts">
// 本地音乐：左侧分组（专辑 / 艺人 / 文件夹，虚拟列表）+ 右侧歌曲（虚拟列表）。
// 桌面端先读后端索引秒开，再后台增量扫描；Android 走 MediaStore。
import { computed, nextTick, onMounted, onUnmounted, ref, shallowRef, watch } from 'vue';
import { useVirtualizer } from '@tanstack/vue-virtual';
import { listen } from '@tauri-apps/api/event';
import { api } from '../lib/api';
import { playList } from '../lib/player';
import { session } from '../lib/session';
import type { LocalLibraryEvent, LocalTrack, Track } from '../lib/types';
import { invalidateLocalLyric } from '../lib/lyrics';
import TrackList from '../components/TrackList.vue';
import CoverThumb from '../components/CoverThumb.vue';
import Icon from '../components/Icon.vue';
import SmSelect from '../components/SmSelect.vue';
import { generateAlbumCovers, isAndroid, nativeListen, queryLocalAudio } from '../lib/native';

type Grouping = 'album' | 'artist' | 'folder';
type SortBy = 'smart' | 'title' | 'artist' | 'album' | 'duration' | 'added';
interface Group {
  key: string;
  name: string;
  sub: string;
  cover: string | null;
  tracks: LocalTrack[];
}

const ALL = '\u0000all';
const SORT_LABELS: Record<SortBy, string> = {
  smart: '默认排序',
  title: '按标题',
  artist: '按歌手',
  album: '按专辑',
  duration: '按时长',
  added: '最近添加',
};
const sorts = Object.keys(SORT_LABELS) as SortBy[];
const GROUP_LABELS: Record<Grouping, string> = { album: '专辑', artist: '艺人', folder: '文件夹' };
const GROUP_ICONS: Record<Grouping, string> = { album: 'album', artist: 'artist', folder: 'folder' };

const all = shallowRef<LocalTrack[]>([]);
/** 曲目 id → 所在文件夹（Android 的播放 path 是 content://，文件夹另存） */
const folders = new Map<string, string>();
const hidden = ref<{ count: number; exts: string[] }>({ count: 0, exts: [] });
const filter = ref('');
const loading = ref(false);
const err = ref('');
const autoNote = ref('');
let autoNoteTimer = 0;
const grouping = ref<Grouping>('album');
const selected = ref(ALL);
const sortBy = ref<SortBy>('smart');
const narrow = ref(false);
const mobileDetail = ref(false);

const collator = new Intl.Collator('zh-Hans-CN-u-co-pinyin', { sensitivity: 'base', numeric: true });

// ---------- 平台可播放格式过滤 ----------
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
/** Android 走 Media3（ExoPlayer），以下格式无内置解码器 */
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
function dirOf(p: string) {
  const i = Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\'));
  return i > 0 ? p.slice(0, i) : p;
}
function baseName(p: string) {
  const i = Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\'));
  return i >= 0 ? p.slice(i + 1) : p;
}

function apply(list: LocalTrack[], filePath: (t: LocalTrack) => string) {
  const out: LocalTrack[] = [];
  const bad = new Set<string>();
  let n = 0;
  folders.clear();
  for (const t of list) {
    const fp = filePath(t);
    const ext = extOf(fp);
    if (!canPlayExt(ext)) {
      n++;
      bad.add(ext);
      continue;
    }
    folders.set(t.id, dirOf(fp));
    out.push(t);
  }
  hidden.value = { count: n, exts: [...bad] };
  all.value = out;
}

function flashNote(msg: string) {
  autoNote.value = msg;
  if (autoNoteTimer) window.clearTimeout(autoNoteTimer);
  autoNoteTimer = window.setTimeout(() => {
    autoNoteTimer = 0;
    autoNote.value = '';
  }, 2500);
}

function applyLyricsChanged(paths: string[]) {
  for (const p of paths) {
    invalidateLocalLyric('local:' + p);
    window.dispatchEvent(new CustomEvent('slim-local-lyrics', { detail: { path: p } }));
  }
}

async function applyCached() {
  try {
    apply(await api.localCached(), (t) => t.path ?? '');
  } catch {
    /* 索引尚未写好 */
  }
}

let scanning = false;

function fingerprint(list: LocalTrack[]) {
  return list.map((t) => [t.id, t.title, t.album, t.cover ?? '', t.durationMs].join('\t')).join('\n');
}

async function scan(opts?: { quiet?: boolean }) {
  if (scanning) return;
  scanning = true;
  const quiet = !!opts?.quiet;
  if (!quiet) loading.value = true;
  if (!quiet) err.value = '';
  const before = quiet ? fingerprint(all.value) : '';
  try {
    if (isAndroid) {
      const list = await queryLocalAudio();
      const fileOf = new Map<string, string>();
      const albumOf = new Map<string, number>();
      const tracks: Track[] = list.map((a) => {
        const id = 'local:' + a.uri;
        fileOf.set(id, a.path || a.uri);
        if (a.albumId != null) albumOf.set(id, a.albumId);
        const tr = a.track ?? 0;
        return {
          id,
          source: 'local',
          title: a.title,
          artists: a.artist ? [a.artist] : [],
          album: a.album,
          durationMs: a.durationMs,
          cover: a.cover || null,
          path: a.uri,
          ncmId: null,
          trackNo: tr % 1000 || null,
          discNo: Math.floor(tr / 1000) || null,
          year: a.year || null,
          addedMs: a.addedMs || null,
        };
      });
      // 拼音 / 艺人拆分失败也要显示曲库：退回到前端最小处理
      let enriched: LocalTrack[];
      try {
        enriched = await api.localEnrich(tracks);
      } catch (e) {
        console.warn('localEnrich failed', e);
        enriched = tracks.map((t) => ({
          ...t,
          searchKey: norm([t.title, ...t.artists, t.album].join(' ')),
          sortKey: norm(t.title),
        }));
      }
      apply(enriched, (t) => fileOf.get(t.id) ?? t.path ?? '');
      // 封面在列表显示后再后台生成，完成后一次性回填
      void generateAlbumCovers().then((covers) => {
        if (!Object.keys(covers).length) return;
        all.value = all.value.map((t) => {
          const aid = albumOf.get(t.id);
          const c = aid != null ? covers[String(aid)] : undefined;
          return c && c !== t.cover ? { ...t, cover: c } : t;
        });
      });
    } else {
      apply(await api.scanLocal(), (t) => t.path ?? '');
    }
  } catch (e) {
    if (!quiet) err.value = String(e);
  } finally {
    scanning = false;
    if (!quiet) loading.value = false;
  }
  return quiet ? fingerprint(all.value) !== before : true;
}

async function init() {
  if (isAndroid) return scan();
  if (!session.settings.localFolders.length) return;
  try {
    const cached = await api.localCached();
    if (cached.length) apply(cached, (t) => t.path ?? '');
  } catch {
    /* 无索引：直接扫描 */
  }
  void scan();
}

// ---------- 分组 ----------
const groups = computed<Group[]>(() => {
  const m = new Map<string, Group>();
  const g = grouping.value;
  const add = (key: string, name: string, sub: string, t: LocalTrack) => {
    let x = m.get(key);
    if (!x) {
      x = { key, name, sub, cover: null, tracks: [] };
      m.set(key, x);
    }
    if (!x.cover && t.cover) x.cover = t.cover;
    x.tracks.push(t);
  };
  for (const t of all.value) {
    if (g === 'album') {
      const who = t.albumArtist || t.artists[0] || '';
      add(`${(t.artistKeys?.[0] ?? who).toLowerCase()}\u0001${t.album.toLowerCase()}`, t.album || '未知专辑', who || '未知歌手', t);
    } else if (g === 'artist') {
      if (!t.artists.length) add('', '未知歌手', '', t);
      // artistKeys：后端已拆分 + 繁→简 + 小写，同一艺人不同写法归为一组
      t.artists.forEach((a, i) => add(t.artistKeys?.[i] ?? a, a, '', t));
    } else {
      const d = folders.get(t.id) ?? '';
      add(d, baseName(d) || d, d, t);
    }
  }
  return [...m.values()].sort((a, b) => collator.compare(a.name, b.name));
});

const groupMap = computed(() => new Map(groups.value.map((g) => [g.key, g])));
const current = computed(() => (selected.value === ALL ? null : groupMap.value.get(selected.value) ?? null));
const title = computed(() => current.value?.name ?? '全部歌曲');

watch(grouping, () => {
  selected.value = ALL;
  void nextTick(() => sideV.value.scrollToOffset(0));
});
watch(groupMap, (m) => {
  if (selected.value !== ALL && !m.has(selected.value)) selected.value = ALL;
});

// ---------- 筛选 + 排序 ----------
function norm(s: string) {
  return s
    .replace(/[\uFF01-\uFF5E]/g, (c) => String.fromCharCode(c.charCodeAt(0) - 0xfee0))
    .replace(/\u3000/g, ' ')
    .toLowerCase();
}
function cmpAlbumOrder(a: Track, b: Track) {
  return (
    (a.discNo ?? 0) - (b.discNo ?? 0) ||
    (a.trackNo ?? 0) - (b.trackNo ?? 0) ||
    0
  );
}
const shown = computed<LocalTrack[]>(() => {
  let list = current.value?.tracks ?? all.value;
  const terms = norm(filter.value).split(/\s+/).filter(Boolean);
  if (terms.length) list = list.filter((t) => terms.every((q) => t.searchKey.includes(q)));
  const s = sortBy.value;
  const byTitle = (a: LocalTrack, b: LocalTrack) =>
    a.sortKey < b.sortKey ? -1 : a.sortKey > b.sortKey ? 1 : 0;
  let cmp: (a: LocalTrack, b: LocalTrack) => number;
  if (s === 'smart') {
    cmp =
      grouping.value === 'album' && current.value
        ? (a, b) => cmpAlbumOrder(a, b) || byTitle(a, b)
        : byTitle;
  } else if (s === 'title') cmp = byTitle;
  else if (s === 'artist')
    cmp = (a, b) => collator.compare(a.artists[0] ?? '', b.artists[0] ?? '') || byTitle(a, b);
  else if (s === 'album')
    cmp = (a, b) => collator.compare(a.album, b.album) || cmpAlbumOrder(a, b) || byTitle(a, b);
  else if (s === 'duration') cmp = (a, b) => a.durationMs - b.durationMs;
  else cmp = (a, b) => (b.addedMs ?? 0) - (a.addedMs ?? 0);
  return list.slice().sort(cmp);
});


// ---------- 左侧虚拟列表 ----------
const sideEl = ref<HTMLElement | null>(null);
const SIDE_ROW = 52;
const sideV = useVirtualizer(
  computed(() => ({
    count: groups.value.length + 1,
    getScrollElement: () => sideEl.value,
    estimateSize: () => SIDE_ROW,
    overscan: 10,
  })),
);
const sideRows = computed(() => sideV.value.getVirtualItems());

function open(key: string) {
  selected.value = key;
  if (narrow.value) mobileDetail.value = true;
}
function backToGroups() {
  mobileDetail.value = false;
}

let mql: MediaQueryList | null = null;
const syncNarrow = () => {
  narrow.value = !!mql?.matches;
  if (!narrow.value) mobileDetail.value = false;
};

let unlistenLib: (() => void) | null = null;
let unlistenNative: (() => void) | null = null;

onMounted(() => {
  mql = window.matchMedia('(max-width: 700px)');
  syncNarrow();
  mql.addEventListener('change', syncNarrow);
  void init();
  void listen<LocalLibraryEvent>('local-library-updated', (e) => {
    const ev = e.payload;
    if (!ev) return;
    if (ev.libraryChanged) {
      const before = fingerprint(all.value);
      void applyCached().then(() => {
        if (fingerprint(all.value) !== before) flashNote('曲库已自动更新');
      });
    }
    if (ev.lyricsChanged?.length) applyLyricsChanged(ev.lyricsChanged);
  }).then((fn) => {
    unlistenLib = fn;
  });
  if (isAndroid) {
    void nativeListen((ne) => {
      if (ne.type !== 'library') return;
      if (loading.value) return;
      void scanQuiet();
    }).then((fn) => {
      unlistenNative = fn;
    });
  }
});
onUnmounted(() => {
  mql?.removeEventListener('change', syncNarrow);
  unlistenLib?.();
  unlistenNative?.();
  if (autoNoteTimer) window.clearTimeout(autoNoteTimer);
});

/** Android MediaStore 变更：静默刷新，不闪「扫描中」。 */
async function scanQuiet() {
  if (scanning || !isAndroid) return;
  try {
    const changed = await scan({ quiet: true });
    if (changed) flashNote('曲库已自动更新');
  } catch {
    /* 权限等 */
  }
}
watch(
  () => session.settings.localFolders.join('\n'),
  () => {
    if (!isAndroid) void scan();
  },
);
</script>

<template>
  <section class="lv" :class="{ 'mobile-detail': mobileDetail }">
    <aside class="side float-panel">
      <div class="side-head">
        <div class="seg" role="group" aria-label="分组">
          <button
            v-for="g in (['album', 'artist', 'folder'] as Grouping[])"
            :key="g"
            type="button"
            :class="{ on: grouping === g }"
            @click="grouping = g"
          >
            <Icon :name="GROUP_ICONS[g]" :size="14" />{{ GROUP_LABELS[g] }}
          </button>
        </div>
      </div>
      <div ref="sideEl" class="side-scroll">
        <Transition name="tracks-swap" mode="out-in">
          <div :key="grouping" class="virt" :style="{ height: sideV.getTotalSize() + 'px' }">
            <div
              v-for="r in sideRows"
              :key="r.index === 0 ? ALL : groups[r.index - 1]?.key"
              class="list-row item"
              :class="{ on: (r.index === 0 ? ALL : groups[r.index - 1]?.key) === selected }"
              :style="{ top: r.start + 'px', height: SIDE_ROW - 4 + 'px' }"
              @click="open(r.index === 0 ? ALL : groups[r.index - 1].key)"
            >
              <template v-if="r.index === 0">
                <span class="gicon"><Icon name="music" :size="16" /></span>
                <span class="name">全部歌曲</span>
                <small class="muted count">{{ all.length }}</small>
              </template>
              <template v-else-if="groups[r.index - 1]">
                <CoverThumb
                  v-if="grouping === 'album'"
                  class="gcover"
                  :src="groups[r.index - 1].cover"
                  :icon-size="14"
                />
                <span v-else class="gicon"><Icon :name="GROUP_ICONS[grouping]" :size="16" /></span>
                <span class="name">
                  {{ groups[r.index - 1].name }}
                  <small v-if="grouping === 'album'" class="muted gsub">{{ groups[r.index - 1].sub }}</small>
                </span>
                <small class="muted count">{{ groups[r.index - 1].tracks.length }}</small>
              </template>
            </div>
          </div>
        </Transition>
      </div>
    </aside>

    <div class="body float-panel">
      <header class="head">
        <button v-if="narrow" class="back btn-ghost" type="button" @click="backToGroups">
          <Icon name="chevron-left" :size="14" /> {{ GROUP_LABELS[grouping] }}
        </button>
        <h2 class="head-title">{{ title }}</h2>
        <input v-model="filter" class="grow" placeholder="筛选（支持拼音 / 首字母）" />
        <div class="head-actions">
          <SmSelect
            class="sort"
            v-model="sortBy"
            :options="sorts"
            :labels="SORT_LABELS"
            icon="sort"
          />
          <button class="btn-primary" :disabled="!shown.length" @click="playList(shown)">播放全部</button>
          <button class="btn-secondary" :disabled="loading" @click="() => scan()">
            {{ loading ? '扫描中…' : isAndroid ? '刷新' : '重新扫描' }}
          </button>
        </div>
      </header>
      <p v-if="!isAndroid && !session.settings.localFolders.length" class="muted note">
        还没有添加文件夹，去「设置」里添加。
      </p>
      <p v-if="autoNote" class="muted note">{{ autoNote }}</p>
      <p v-if="hidden.count" class="muted note">
        已隐藏 {{ hidden.count }} 首当前平台无法播放的文件（{{ hidden.exts.join(' / ') }}）
      </p>
      <p v-if="err" class="error">{{ err }}</p>
      <Transition name="tracks-swap" mode="out-in">
        <div :key="grouping + selected" class="tracks-scroll">
          <TrackList :tracks="shown" />
        </div>
      </Transition>
    </div>
  </section>
</template>

<style scoped>
.lv {
  display: grid;
  grid-template-columns: var(--side-w) minmax(0, 1fr);
  gap: var(--chrome-gap);
  min-width: 0;
  min-height: 0;
  flex: 1;
  height: 100%;
  background: transparent;
  overflow: visible;
}
.float-panel {
  background: var(--panel-bg);
  border: var(--panel-border);
  border-radius: var(--panel-radius);
  box-shadow: var(--panel-shadow);
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  isolation: isolate;
}
.side {
  display: flex;
  flex-direction: column;
  padding: var(--space-3);
}
.side-head {
  margin-bottom: var(--space-2);
}
.side-head .seg {
  width: 100%;
}
.side-head .seg button {
  flex: 1;
  padding: 0 var(--space-1);
}
.side-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}
.virt {
  position: relative;
  width: 100%;
}
.item {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  box-sizing: border-box;
  gap: var(--space-2);
}
.gcover {
  width: 32px;
  height: 32px;
}
.gicon {
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  display: grid;
  place-items: center;
  border-radius: var(--radius);
  background: var(--fill-secondary);
  color: var(--muted);
}
.item .name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  line-height: 1.25;
}
.gsub {
  font-size: 11px;
  font-weight: 450;
  overflow: hidden;
  text-overflow: ellipsis;
}
.item .count {
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
}
.item.on .count {
  color: var(--text-secondary);
}
.body {
  display: flex;
  flex-direction: column;
  padding: var(--space-3) var(--space-4);
  overflow: hidden;
}
.head .head-title {
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.sort :deep(.sm-select-trigger) {
  gap: var(--space-1);
}
.note {
  font-size: 13px;
  margin: 0 0 var(--space-2);
}
.tracks-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.back {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  color: var(--muted);
  flex-shrink: 0;
}

@media (max-width: 700px) {
  .lv {
    grid-template-columns: minmax(0, 1fr);
    gap: 0;
  }
  .lv:not(.mobile-detail) .body {
    display: none;
  }
  .lv.mobile-detail .side {
    display: none;
  }
  .lv.mobile-detail .body {
    animation: sm-fade-in var(--dur) var(--ease-enter);
  }
}

.tracks-swap-enter-active,
.tracks-swap-leave-active {
  transition:
    opacity var(--dur) var(--ease-swift),
    transform var(--dur) var(--ease-swift);
}
.tracks-swap-enter-from {
  opacity: 0;
  transform: translateX(10px);
}
.tracks-swap-leave-to {
  opacity: 0;
  transform: translateX(-8px);
}
</style>
