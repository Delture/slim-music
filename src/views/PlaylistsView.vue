<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue';
import { api } from '../lib/api';
import { playList } from '../lib/player';
import { session } from '../lib/session';
import type { Playlist, Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';
import Icon from '../components/Icon.vue';

const lists = ref<Playlist[]>([]);
const active = ref<Playlist | null>(null);
const tracks = ref<Track[]>([]);
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
      await open(keep, false, !narrow.value);
    } else {
      active.value = null;
      tracks.value = [];
      mobileDetail.value = false;
    }
    if (!refresh && hit.fromCache) {
      void api.myPlaylists(true).then((h) => {
        lists.value = h.data;
        fromCache.value = false;
      });
    }
  } catch (e) {
    err.value = String(e);
  }
}

async function open(p: Playlist, doBackgroundRefresh = true, enterDetail = true) {
  active.value = p;
  if (enterDetail && narrow.value) mobileDetail.value = true;
  loading.value = true;
  err.value = '';
  try {
    const hit = await api.playlistTracks(p.id, false);
    if (active.value?.id !== p.id) return;
    tracks.value = hit.data;
    fromCache.value = hit.fromCache;
    loading.value = false;
    if (doBackgroundRefresh && (hit.fromCache || hit.ageSecs > 0)) {
      void api.playlistTracks(p.id, true).then((h) => {
        if (active.value?.id === p.id) {
          tracks.value = h.data;
          fromCache.value = false;
        }
      });
    }
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

async function menuUnsubscribe() {
  const p = menu.target;
  closeMenu();
  if (!p || !p.subscribed || p.isLiked) return;
  if (!confirm(`确定取消收藏「${p.name}」？`)) return;
  busy.value = true;
  try {
    await api.playlistSubscribe(p.id, false);
    if (active.value?.id === p.id) mobileDetail.value = false;
    await loadLists(true);
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
  }
}

async function menuDelete() {
  const p = menu.target;
  closeMenu();
  if (!p || !p.owned || p.isLiked) return;
  if (
    !confirm(
      `确定删除网易云歌单「${p.name}」？\n此操作不可恢复，歌单内歌曲不会从云盘删除。`,
    )
  ) {
    return;
  }
  busy.value = true;
  try {
    await api.playlistDelete(p.id);
    if (active.value?.id === p.id) {
      active.value = null;
      mobileDetail.value = false;
    }
    await loadLists(true);
  } catch (e) {
    alert(String(e));
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
      <div class="tracks-scroll">
        <TrackList :tracks="tracks" />
      </div>
    </div>

    <div
      v-if="menu.show && menu.target"
      class="ctx"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
      @click.stop
    >
      <div @click="menuPlayAll">播放全部</div>
      <div v-if="menuCanUnsubscribe" :class="{ disabled: busy }" @click="menuUnsubscribe">取消收藏</div>
      <div v-if="menuCanDelete" class="danger" :class="{ disabled: busy }" @click="menuDelete">删除歌单</div>
    </div>
  </section>
</template>

<style scoped>
/* Four-panel shell: this view owns middle (list) + right (tracks) as TWO detached float panels. */
.pl {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
  gap: var(--chrome-gap);
  min-width: 0;
  min-height: 0;
  flex: 1;
  height: 100%;
  background: transparent;
}
.float-panel {
  background: var(--panel-bg);
  border: var(--panel-border);
  border-radius: var(--panel-radius);
  box-shadow: var(--panel-shadow);
  min-width: 0;
  min-height: 0;
}
.side {
  padding: var(--space-3);
  overflow: auto;
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
}
.item .count {
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
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
  padding: 6px;
  box-shadow: var(--shadow-lg);
  z-index: 40;
  min-width: 160px;
  animation: sm-scale-in var(--dur) var(--ease-enter);
  transform-origin: top left;
}
.ctx div {
  padding: 10px 14px;
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
</style>
