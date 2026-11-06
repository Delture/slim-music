<script setup lang="ts">
import { dialog } from '../lib/dialog';
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue';
import { api } from '../lib/api';
import { playList } from '../lib/player';
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

const menuCanUnsubscribe = computed(
  () => !!menu.target && menu.target.subscribed && !menu.target.isLiked,
);
const menuCanDelete = computed(
  () => !!menu.target && menu.target.owned && !menu.target.isLiked,
);

const shown = computed(() => filterTracks(tracks.value, filter.value));

/** 后台补拼音搜索键（保留歌单原有顺序，含「我喜欢的音乐」收藏时间序）；失败不影响列表显示 */
async function enrichInPlace(list: Track[], playlistId: number) {
  if (!list.length) return;
  try {
    const enriched = await api.localEnrich(list);
    if (active.value?.id === playlistId) tracks.value = enriched;
  } catch {
    /* ignore */
  }
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
    const h = await api.playlistTracks(p.id, true, p.isLiked);
    if (active.value?.id === p.id) {
      tracks.value = h.data.slice();
      fromCache.value = false;
      void enrichInPlace(h.data, p.id);
    }
  } catch {
    /* 保留缓存 */
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
  loading.value = true;
  err.value = '';
  try {
    const hit = await api.playlistTracks(p.id, false, p.isLiked);
    if (active.value?.id !== p.id) return;
    tracks.value = hit.data;
    fromCache.value = hit.fromCache;
    loading.value = false;
    void enrichInPlace(hit.data, p.id);
    if (doBackgroundRefresh && (hit.fromCache || hit.ageSecs > 0)) void refreshTracks(p);
  } catch (e) {
    if (active.value?.id === p.id) {
      err.value = String(e);
      loading.value = false;
    }
  }
}

function backToLists() {
  mobileDetail.value = false;
}

function closeMenu() {
  menu.show = false;
  menu.target = null;
}

async function openMenu(e: MouseEvent, p: Playlist) {
  Object.assign(menu, { show: true, x: e.clientX, y: e.clientY, target: p });
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
    if (active.value?.id === p.id) {
      active.value = null;
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
onUnmounted(() => {
  mql?.removeEventListener('change', syncNarrow);
});
</script>

<template>
  <section class="pl" :class="{ 'mobile-detail': mobileDetail }" @click="closeMenu">
    <div v-if="!session.profile" class="float-panel login-hint muted">请先在「设置」中扫码登录网易云。</div>
    <aside v-else class="side float-panel">
      <div class="side-title">我的歌单</div>
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
          <TrackList :tracks="shown" />
        </div>
      </Transition>
    </div>

    <div
      v-if="menu.show && menu.target"
      class="ctx"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
      @click.stop
    >
      <div @click="menuPlayAll">播放全部</div>
      <div :class="{ disabled: busy }" @click="menuRefresh">刷新</div>
      <div v-if="menuCanUnsubscribe" :class="{ disabled: busy }" @click="menuUnsubscribe">取消收藏</div>
      <div v-if="menuCanDelete" class="danger" :class="{ disabled: busy }" @click="menuDelete">删除歌单</div>
    </div>
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
.side-title {
  display: none;
  font-family: var(--font-display);
  font-weight: 500;
  font-size: 24px;
  margin-bottom: var(--space-3);
  letter-spacing: -0.02em;
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
  z-index: 40;
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
  .side-title {
    display: block;
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
