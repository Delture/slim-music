<script setup lang="ts">
import { dialog } from '../lib/dialog';
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue';
import { useAppMenuDismiss } from '../lib/appMenu';
import { api } from '../lib/api';
import { playList } from '../lib/player';
import {
  forgetCachedPlaylistTracks,
  playlistCacheEpoch,
  setCachedPlaylistTracks,
  usableCachedPlaylistTracks,
} from '../lib/playlistCache';
import { resumeSync, session } from '../lib/session';
import type { Playlist, Track } from '../lib/types';
import { filterTracks } from '../lib/trackFilter';
import TrackList from '../components/TrackList.vue';
import TrackFilter from '../components/TrackFilter.vue';
import Icon from '../components/Icon.vue';

const lists = ref<Playlist[]>([]);
const active = ref<Playlist | null>(null);
const tracks = ref<Track[]>([]);
const filter = ref('');
const err = ref('');
const loading = ref(false);
const busy = ref(false);
const fromCache = ref(false);
/** 窄屏：列表 ↔ 详情 叠层导航 */
const mobileDetail = ref(false);
const narrow = ref(false);
let mql: MediaQueryList | null = null;

const menu = reactive({
  show: false,
  x: 0,
  y: 0,
  target: null as Playlist | null,
});
const menuEl = ref<HTMLElement | null>(null);
const { openMenuAt, closeMenu: hideMenu } = useAppMenuDismiss(menu, menuEl);

const menuCanUnsubscribe = computed(
  () => !!menu.target && menu.target.subscribed && !menu.target.isLiked,
);
const menuCanDelete = computed(
  () => !!menu.target && menu.target.owned && !menu.target.isLiked,
);

const shown = computed(() => filterTracks(tracks.value, filter.value));

/** 后台补拼音搜索键（保留歌单原有顺序，含「我喜欢的音乐」收藏时间序）；失败不影响列表显示 */
function sameTrackIds(a: Track[], b: Track[]) {
  return a.length === b.length && a.every((t, i) => t.id === b[i]?.id);
}

async function enrichInPlace(list: Track[], playlistId: number) {
  if (!list.length) return;
  try {
    const enriched = await api.localEnrich(list);
    if (active.value?.id !== playlistId) return;
    // 后台刷新若已换了更新的曲目，不要用旧列表的拼音结果覆盖
    if (tracks.value !== list && !sameTrackIds(tracks.value, list)) return;
    tracks.value = enriched;
    setCachedPlaylistTracks(playlistId, enriched);
  } catch {
    /* ignore */
  }
}

function tracksUid(p: Playlist): number | undefined {
  return p.creatorId || session.profile?.userId || undefined;
}

function rememberTracks(p: Playlist, list: Track[]) {
  // 预期有曲目却拿到空列表：不要把空结果当成有效缓存
  if (!list.length && p.trackCount > 0) {
    forgetCachedPlaylistTracks(p.id);
    return;
  }
  setCachedPlaylistTracks(p.id, list);
}

function showTracks(p: Playlist, list: Track[], cached: boolean) {
  if (active.value?.id !== p.id) return;
  tracks.value = list;
  fromCache.value = cached;
  loading.value = false;
}

const inflight = new Map<number, Promise<void>>();

function loadTracks(p: Playlist, refresh: boolean, backgroundRefresh: boolean) {
  const existing = !refresh ? inflight.get(p.id) : undefined;
  if (existing) return existing;
  let req!: Promise<void>;
  req = (async () => {
    try {
      const hit = await api.playlistTracks(p.id, refresh, p.isLiked, tracksUid(p));
      const unexpectedEmpty = !hit.data.length && p.trackCount > 0;
      if (unexpectedEmpty) {
        forgetCachedPlaylistTracks(p.id);
        if (!refresh && backgroundRefresh) {
          await refreshTracks(p);
          return;
        }
        if (active.value?.id === p.id) showTracks(p, [], false);
        return;
      }
      rememberTracks(p, hit.data);
      if (active.value?.id === p.id) {
        showTracks(p, hit.data, hit.fromCache);
        void enrichInPlace(hit.data, p.id);
        if (backgroundRefresh && !refresh && (hit.fromCache || hit.ageSecs > 0)) {
          void refreshTracks(p);
        }
      }
    } catch (e) {
      if (active.value?.id === p.id) {
        err.value = String(e);
        loading.value = false;
      }
    } finally {
      if (inflight.get(p.id) === req) inflight.delete(p.id);
    }
  })();
  if (!refresh) inflight.set(p.id, req);
  return req;
}

function syncNarrow() {
  narrow.value = !!mql?.matches;
  if (!narrow.value) mobileDetail.value = false;
}

async function loadLists(refresh = false) {
  err.value = '';
  try {
    const hit = await api.myPlaylists(refresh);
    lists.value = hit.data;
    fromCache.value = hit.fromCache;
    const keep =
      lists.value.find((l) => l.id === active.value?.id) ||
      lists.value.find((l) => l.isLiked) ||
      lists.value[0];
    if (keep) {
      // 窄屏只预选，不自动钻进详情，避免侧栏占满宽度
      await open(keep, true, !narrow.value);
    } else {
      active.value = null;
      tracks.value = [];
      mobileDetail.value = false;
    }
    if (!refresh && hit.fromCache) {
      void api.myPlaylists(true).then((h) => applyFreshLists(h.data)).catch(() => {});
    }
  } catch (e) {
    err.value = String(e);
  }
}

/** 新歌单列表到达：替换列表；若当前歌单 trackCount 变化则重拉曲目 */
function applyFreshLists(fresh: Playlist[]) {
  const cur = active.value;
  lists.value = fresh;
  fromCache.value = false;
  if (!cur) return;
  const next = fresh.find((l) => l.id === cur.id);
  if (!next) return;
  active.value = next;
  if (next.trackCount !== cur.trackCount || next.trackCount !== tracks.value.length) {
    void refreshTracks(next);
  }
}

/** 强制从网络拉取曲目并替换 UI（新数组，保证虚拟列表重渲染） */
async function refreshTracks(p: Playlist) {
  try {
    const h = await api.playlistTracks(p.id, true, p.isLiked, tracksUid(p));
    if (!h.data.length && p.trackCount > 0) {
      forgetCachedPlaylistTracks(p.id);
      if (active.value?.id === p.id && !tracks.value.length) showTracks(p, [], false);
      return;
    }
    const list = h.data.slice();
    rememberTracks(p, list);
    if (active.value?.id === p.id) {
      showTracks(p, list, false);
      void enrichInPlace(list, p.id);
    }
  } catch (e) {
    // 已有曲目时保留画面；首次仍是空的则露出错误并结束「加载中…」
    if (active.value?.id === p.id && !tracks.value.length) {
      err.value = String(e);
      loading.value = false;
    }
  }
}

/** 静默刷新：歌单列表 + 当前歌单曲目 */
async function syncNow() {
  if (!session.profile) return;
  try {
    applyFreshLists((await api.myPlaylists(true)).data);
  } catch {
    /* ignore */
  }
  if (active.value) await refreshTracks(active.value);
}

async function open(p: Playlist, doBackgroundRefresh = true, enterDetail = true) {
  const switched = active.value?.id !== p.id;
  active.value = p;
  if (switched) filter.value = '';
  if (enterDetail && narrow.value) mobileDetail.value = true;
  err.value = '';

  // 先换壳并开始过渡，不把动画和点击卡在曲目接口上。
  const cached = usableCachedPlaylistTracks(p.id, p.trackCount);
  if (cached) {
    showTracks(p, cached, true);
    void enrichInPlace(cached, p.id);
    if (doBackgroundRefresh) void refreshTracks(p);
    return;
  }

  if (switched) tracks.value = [];
  loading.value = true;
  await loadTracks(p, false, doBackgroundRefresh);
}

function backToLists() {
  mobileDetail.value = false;
}

function closeMenu() {
  hideMenu();
  menu.target = null;
}

async function openMenu(e: MouseEvent, p: Playlist) {
  menu.target = p;
  openMenuAt(e);
}

async function onCreate() {
  const name = await dialog.prompt('新建歌单', '新歌单');
  if (!name?.trim() || busy.value) return;
  busy.value = true;
  try {
    const id = await api.playlistCreate(name.trim());
    const hit = await api.myPlaylists(true);
    lists.value = hit.data;
    fromCache.value = false;
    const created = lists.value.find((l) => l.id === id);
    if (created) await open(created, false, true);
  } catch (e) {
    void dialog.alert(String(e));
  } finally {
    busy.value = false;
  }
}

async function menuPlayAll() {
  const p = menu.target;
  closeMenu();
  if (!p) return;
  // Ensure tracks for that playlist
  if (active.value?.id !== p.id) await open(p, false, false);
  if (tracks.value.length) playList(tracks.value);
}

async function menuRefresh() {
  const p = menu.target;
  closeMenu();
  if (!p || busy.value) return;
  busy.value = true;
  try {
    if (active.value?.id !== p.id) await open(p, false, false);
    await syncNow();
  } finally {
    busy.value = false;
  }
}

async function menuUnsubscribe() {
  const p = menu.target;
  closeMenu();
  if (!p || !p.subscribed || p.isLiked) return;
  if (!(await dialog.confirm('取消收藏', `「${p.name}」`, { danger: true, confirmText: '取消收藏' }))) return;
  busy.value = true;
  try {
    await api.playlistSubscribe(p.id, false);
    forgetCachedPlaylistTracks(p.id);
    if (active.value?.id === p.id) mobileDetail.value = false;
    await loadLists(true);
  } catch (e) {
    void dialog.alert(String(e));
  } finally {
    busy.value = false;
  }
}

async function menuDelete() {
  const p = menu.target;
  closeMenu();
  if (!p || !p.owned || p.isLiked) return;
  if (!(await dialog.confirm('删除歌单', `「${p.name}」
此操作不可恢复`, { danger: true, confirmText: '删除' }))) return;
  busy.value = true;
  try {
    await api.playlistDelete(p.id);
    forgetCachedPlaylistTracks(p.id);
    if (active.value?.id === p.id) {
      active.value = null;
      tracks.value = [];
      mobileDetail.value = false;
    }
    await loadLists(true);
  } catch (e) {
    void dialog.alert(String(e));
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  mql = window.matchMedia('(max-width: 700px)');
  syncNarrow();
  mql.addEventListener('change', syncNarrow);
  if (session.profile) void loadLists(false);
});
watch(() => resumeSync.tick, () => void syncNow());
watch(playlistCacheEpoch, () => {
  if (!session.profile || !active.value) return;
  tracks.value = [];
  loading.value = true;
  void open(active.value, true, false);
});
onUnmounted(() => {
  mql?.removeEventListener('change', syncNarrow);
});
</script>

<template>
  <section class="pl" :class="{ 'mobile-detail': mobileDetail }">
    <div v-if="!session.profile" class="float-panel login-hint muted">请先在「设置」中扫码登录网易云。</div>
    <aside v-else class="side float-panel">
      <div class="side-head">
        <span>我的歌单</span>
        <button class="btn-icon link" type="button" title="新建歌单" @click="onCreate">
          <Icon name="plus" :size="16" />
        </button>
      </div>
      <div
        v-for="p in lists"
        :key="p.id"
        class="list-row item"
        :class="{ on: active?.id === p.id }"
        @click="open(p)"
        @contextmenu.prevent="openMenu($event, p)"
      >
        <span class="name">{{ p.isLiked ? '我喜欢的音乐' : p.name }}</span>
        <small class="muted count">{{ p.trackCount }}</small>
      </div>
    </aside>
    <div v-if="active" class="body float-panel">
      <!-- Mobile only: back + tiny status. No large title / action buttons — selection in middle panel is enough. -->
      <header v-if="narrow" class="mobile-bar">
        <button class="back btn-ghost" type="button" @click="backToLists">
          <Icon name="chevron-left" :size="14" /> 歌单
        </button>
        <span v-if="fromCache" class="sm-badge muted-chip">缓存</span>
        <span v-if="loading && !tracks.length" class="muted tiny">加载中…</span>
      </header>
      <p v-if="err" class="error">{{ err }}</p>
      <TrackFilter v-model="filter" />
      <!-- Soft fade/slide when switching selected playlist (not only row highlight). -->
      <Transition name="tracks-swap" mode="out-in">
        <p
          v-if="filter.trim() && !shown.length"
          :key="'empty-' + active.id"
          class="muted empty-hint"
        >没有匹配「{{ filter.trim() }}」的歌曲</p>
        <div v-else :key="active.id" class="tracks-scroll">
          <p v-if="loading && !tracks.length" class="muted empty-hint">加载中…</p>
          <TrackList v-else :tracks="shown" />
        </div>
      </Transition>
    </div>

    <Teleport to="body">
    <div
      v-if="menu.show && menu.target"
      ref="menuEl"
      class="ctx"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
      @click.stop
      @pointerdown.stop
    >
      <div @click="menuPlayAll">播放全部</div>
      <div :class="{ disabled: busy }" @click="menuRefresh">刷新</div>
      <div v-if="menuCanUnsubscribe" :class="{ disabled: busy }" @click="menuUnsubscribe">取消收藏</div>
      <div v-if="menuCanDelete" class="danger" :class="{ disabled: busy }" @click="menuDelete">删除歌单</div>
    </div>
    </Teleport>
  </section>
</template>

<style scoped>
/* Four-panel shell: this view owns middle (list) + right (tracks) as TWO detached float panels. */
.pl {
  display: grid;
  grid-template-columns: var(--side-w) minmax(0, 1fr);
  gap: var(--chrome-gap); /* window bg must show between list + tracks */
  min-width: 0;
  min-height: 0;
  flex: 1;
  height: 100%;
  background: transparent;
  overflow: visible; /* never square-clip child .float-panel radii */
}
.float-panel {
  background: var(--panel-bg);
  border: var(--panel-border);
  border-radius: var(--panel-radius);
  box-shadow: var(--panel-shadow);
  min-width: 0;
  min-height: 0;
  overflow: hidden; /* clip to own radius — parent must stay overflow:visible */
  isolation: isolate;
}
.side {
  padding: var(--space-3);
  overflow-x: hidden;
  overflow-y: auto; /* scroll inside rounded panel */
}
.body {
  display: flex;
  flex-direction: column;
  padding: var(--space-3) var(--space-4);
  overflow: hidden;
}
.side-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-family: var(--font-display);
  font-weight: 500;
  font-size: 17px;
  letter-spacing: -0.015em;
  margin-bottom: var(--space-3);
  min-height: var(--btn-h);
  padding: 0 var(--row-px);
}
.link {
  color: var(--accent);
  background: transparent;
}
.link:hover:not(:disabled) {
  background: var(--hover);
  color: var(--accent);
}
.tracks-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.empty-hint {
  padding: var(--empty-pad);
  text-align: center;
  font-size: 14px;
}
.item {
  justify-content: space-between;
}
.item .name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  min-width: 0;
  flex: 1;
  transition: color var(--dur-fast) var(--ease-swift);
}
.item .count {
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
  transition: color var(--dur-fast) var(--ease-swift);
}
.item.on .count {
  color: var(--text-secondary);
}
.login-hint {
  grid-column: 1 / -1;
  padding: var(--space-5);
  display: flex;
  align-items: center;
}
.mobile-bar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-2);
  min-height: var(--btn-h);
}
.back {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  color: var(--muted);
  flex-shrink: 0;
}
.tiny {
  font-size: 12px;
}
.ctx {
  position: fixed;
  background: var(--surface-3);
  border: 0.5px solid var(--border);
  border-radius: var(--radius-lg);
  padding: var(--menu-pad);
  box-shadow: var(--elev-2);
  z-index: 9000;
  min-width: 160px;
  animation: sm-scale-in var(--dur) var(--ease-enter);
  transform-origin: top left;
  -webkit-backdrop-filter: none;
  backdrop-filter: none;
}
.ctx div {
  padding: var(--menu-item-py) var(--menu-item-px);
  border-radius: var(--radius);
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
  transition: background var(--dur-fast) var(--ease-swift);
}
.ctx div:hover {
  background: var(--fill-secondary);
}
.ctx .danger {
  color: var(--btn-danger-fg);
}
.ctx .disabled {
  opacity: 0.45;
  pointer-events: none;
}

@media (max-width: 700px) {
  .pl {
    grid-template-columns: minmax(0, 1fr);
    gap: 0;
  }
  .side {
    max-height: none;
  }
  .side-head {
    font-size: 24px;
    letter-spacing: -0.02em;
    padding: 0;
  }
  .item {
    min-height: 48px;
    font-size: 15px;
  }
  .pl:not(.mobile-detail) .body {
    display: none;
  }
  .pl.mobile-detail .side {
    display: none;
  }
  .pl.mobile-detail .body {
    animation: sm-fade-in var(--dur) var(--ease-enter);
  }
}

/* Playlist → tracks content soft swap */
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
