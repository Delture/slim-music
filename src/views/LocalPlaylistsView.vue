<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue';
import { playList, player } from '../lib/player';
import {
  createLocalPlaylist,
  deleteLocalPlaylist,
  localPl,
  moveLocalTrack,
  refreshLocalPlaylists,
  renameLocalPlaylist,
} from '../lib/localPlaylists';
import type { LocalPlaylist, Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';
import Icon from '../components/Icon.vue';

/** 特殊 id：镜像当前播放队列，不落盘 */
const NOW_PLAYING = '__now_playing__';

const activeId = ref<string>(NOW_PLAYING);
const err = ref('');
/** 窄屏：列表 ↔ 详情 叠层导航 */
const mobileDetail = ref(false);
const narrow = ref(false);
let mql: MediaQueryList | null = null;

const menu = reactive({
  show: false,
  x: 0,
  y: 0,
  /** null = 正在播放 row; otherwise a user playlist */
  target: null as LocalPlaylist | null,
  isNow: false,
});

const nowPlayingTracks = computed<Track[]>(() => {
  if (!player.order.length) return [...player.queue];
  return player.order.map((i) => player.queue[i]).filter(Boolean) as Track[];
});

const isNowPlaying = computed(() => activeId.value === NOW_PLAYING);

const activeUser = computed(
  () => localPl.lists.find((p) => p.id === activeId.value) ?? null,
);

const activeTracks = computed(() =>
  isNowPlaying.value ? nowPlayingTracks.value : activeUser.value?.tracks ?? [],
);

function syncNarrow() {
  narrow.value = !!mql?.matches;
  if (!narrow.value) mobileDetail.value = false;
}

async function ensure() {
  try {
    await refreshLocalPlaylists();
  } catch (e) {
    err.value = String(e);
  }
}

function openNow() {
  activeId.value = NOW_PLAYING;
  if (narrow.value) mobileDetail.value = true;
}
function open(p: LocalPlaylist) {
  activeId.value = p.id;
  if (narrow.value) mobileDetail.value = true;
}
function backToLists() {
  mobileDetail.value = false;
}

function closeMenu() {
  menu.show = false;
  menu.target = null;
  menu.isNow = false;
}

function openMenuNow(e: MouseEvent) {
  Object.assign(menu, { show: true, x: e.clientX, y: e.clientY, target: null, isNow: true });
}

function openMenuPl(e: MouseEvent, p: LocalPlaylist) {
  Object.assign(menu, { show: true, x: e.clientX, y: e.clientY, target: p, isNow: false });
}

function menuPlayAll() {
  const tracks = menu.isNow
    ? nowPlayingTracks.value
    : menu.target?.tracks ?? [];
  closeMenu();
  if (tracks.length) playList(tracks);
}

async function onCreate() {
  const name = prompt('新建播放列表名称', '新播放列表');
  if (!name?.trim()) return;
  try {
    const pl = await createLocalPlaylist(name.trim());
    activeId.value = pl.id;
    if (narrow.value) mobileDetail.value = true;
  } catch (e) {
    alert(String(e));
  }
}

async function menuRename() {
  const p = menu.target;
  closeMenu();
  if (!p) return;
  const name = prompt('重命名', p.name);
  if (!name?.trim() || name.trim() === p.name) return;
  try {
    await renameLocalPlaylist(p.id, name.trim());
  } catch (e) {
    alert(String(e));
  }
}

async function menuDelete() {
  const p = menu.target;
  closeMenu();
  if (!p) return;
  if (!confirm(`确定删除播放列表「${p.name}」？歌曲文件不会被删除。`)) return;
  try {
    await deleteLocalPlaylist(p.id);
    if (activeId.value === p.id) {
      activeId.value = NOW_PLAYING;
      if (narrow.value) mobileDetail.value = false;
    }
  } catch (e) {
    alert(String(e));
  }
}

async function onReorder(e: { from: number; to: number }) {
  if (isNowPlaying.value || !activeUser.value) return;
  try {
    await moveLocalTrack(activeUser.value.id, e.from, e.to);
  } catch (err) {
    alert(String(err));
  }
}

onMounted(() => {
  mql = window.matchMedia('(max-width: 700px)');
  syncNarrow();
  mql.addEventListener('change', syncNarrow);
  void ensure();
});
onUnmounted(() => {
  mql?.removeEventListener('change', syncNarrow);
});
watch(
  () => localPl.lists.map((p) => p.id).join(','),
  () => {
    if (
      activeId.value !== NOW_PLAYING &&
      !localPl.lists.some((p) => p.id === activeId.value)
    ) {
      activeId.value = NOW_PLAYING;
      if (narrow.value) mobileDetail.value = false;
    }
  },
);
</script>

<template>
  <section class="pl" :class="{ 'mobile-detail': mobileDetail }" @click="closeMenu">
    <aside class="side float-panel">
      <div class="side-head">
        <span>播放列表</span>
        <button class="btn-icon link" type="button" title="新建" @click="onCreate">
          <Icon name="plus" :size="16" />
        </button>
      </div>

      <div
        class="list-row item"
        :class="{ on: isNowPlaying }"
        @click="openNow"
        @contextmenu.prevent="openMenuNow($event)"
      >
        <span class="name">正在播放</span>
        <small class="muted count">{{ nowPlayingTracks.length }}</small>
      </div>

      <div class="sep"></div>

      <div
        v-for="p in localPl.lists"
        :key="p.id"
        class="list-row item"
        :class="{ on: activeId === p.id }"
        @click="open(p)"
        @contextmenu.prevent="openMenuPl($event, p)"
      >
        <span class="name" :title="p.name">{{ p.name }}</span>
        <small class="muted count">{{ p.tracks.length }}</small>
      </div>
    </aside>

    <div class="body float-panel">
      <!-- Mobile only: back. No large title / action buttons — middle selection is enough. -->
      <header v-if="narrow" class="mobile-bar">
        <button class="back btn-ghost" type="button" @click="backToLists">
          <Icon name="chevron-left" :size="14" /> 列表
        </button>
      </header>
      <p v-if="err" class="error">{{ err }}</p>
      <p v-if="isNowPlaying && !activeTracks.length" class="muted empty-hint">队列为空，去别处点一首歌开始听</p>
      <div v-else class="tracks-scroll">
        <TrackList
          :tracks="activeTracks"
          :local-playlist-id="isNowPlaying ? null : activeUser?.id"
          :show-reorder="!isNowPlaying && !!activeUser"
          @reorder="onReorder"
        />
      </div>
    </div>

    <div
      v-if="menu.show"
      class="ctx"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
      @click.stop
    >
      <div @click="menuPlayAll">播放全部</div>
      <template v-if="menu.target">
        <div @click="menuRename">重命名</div>
        <div class="danger" @click="menuDelete">删除歌单</div>
      </template>
    </div>
  </section>
</template>

<style scoped>
/* Four-panel shell: middle (list) + right (tracks) as TWO detached float panels. */
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
.sep {
  height: 0;
  border-top: var(--panel-divider);
  margin: var(--space-2) var(--row-px);
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
.empty-hint {
  padding: var(--space-5) var(--space-3);
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
</style>
