<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
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

const activeTitle = computed(() =>
  isNowPlaying.value ? '正在播放' : activeUser.value?.name ?? '',
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

async function onRename(p: LocalPlaylist) {
  const name = prompt('重命名', p.name);
  if (!name?.trim() || name.trim() === p.name) return;
  try {
    await renameLocalPlaylist(p.id, name.trim());
  } catch (e) {
    alert(String(e));
  }
}

async function onDelete(p: LocalPlaylist) {
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
  <section class="pl" :class="{ 'mobile-detail': mobileDetail }">
    <aside class="side panel">
      <div class="side-head">
        <span>播放列表</span>
        <button class="btn-icon link" type="button" title="新建" @click="onCreate">
          <Icon name="plus" :size="16" />
        </button>
      </div>

      <div class="list-row item" :class="{ on: isNowPlaying }" @click="openNow">
        <span class="name">正在播放</span>
        <small class="muted count">{{ nowPlayingTracks.length }}</small>
      </div>

      <div class="sep"></div>

      <div
        v-for="p in localPl.lists"
        :key="p.id"
        class="list-row item has-actions"
        :class="{ on: activeId === p.id }"
        @click="open(p)"
      >
        <span class="name" :title="p.name" @dblclick.stop="onRename(p)">{{ p.name }}</span>
        <small class="muted count">{{ p.tracks.length }}</small>
        <button class="btn-icon mini" type="button" title="重命名" @click.stop="onRename(p)"><Icon name="pencil" :size="13" /></button>
        <button class="btn-icon mini" type="button" title="删除" @click.stop="onDelete(p)"><Icon name="x" :size="14" /></button>
      </div>
    </aside>

    <div class="body panel">
      <header class="head">
        <button v-if="narrow" class="back btn-ghost" type="button" @click="backToLists">
          <Icon name="chevron-left" :size="14" /> 列表
        </button>
        <h2 class="head-title title-ellip">{{ activeTitle }}</h2>
        <div class="head-actions">
          <button
            v-if="activeTracks.length && !isNowPlaying"
            class="btn-primary"
            @click="playList(activeTracks)"
          >
            播放全部
          </button>
          <template v-if="activeUser">
            <button class="btn-ghost" @click="onRename(activeUser)">重命名</button>
            <button class="btn-danger" @click="onDelete(activeUser)">删除</button>
          </template>
        </div>
      </header>
      <p v-if="err" class="error">{{ err }}</p>
      <p v-if="isNowPlaying && !activeTracks.length" class="muted">队列为空，去别处点一首歌开始听</p>
      <TrackList
        v-else
        :tracks="activeTracks"
        :local-playlist-id="isNowPlaying ? null : activeUser?.id"
        :show-reorder="!isNowPlaying && !!activeUser"
        @reorder="onReorder"
      />
    </div>
  </section>
</template>

<style scoped>
/* Co-visible panes share ONE --panel-* token (same fill/rim). Divider only. */
.pl {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
  gap: 0;
  min-width: 0;
  background: var(--panel-bg);
  border-radius: var(--panel-radius);
}
.panel {
  background: var(--panel-bg);
  border: 0;
  box-shadow: var(--panel-shadow);
  min-width: 0;
}
.side {
  border-right: var(--panel-divider);
  padding: 0 var(--space-2) 0 0;
  max-height: calc(100vh - 140px);
  overflow: auto;
}
.body {
  padding: 0 0 0 var(--space-4);
  min-width: 0;
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
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--space-1);
}
.item.has-actions {
  grid-template-columns: minmax(0, 1fr) auto auto auto;
}
.item .name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  min-width: 0;
}
.item .count {
  font-variant-numeric: tabular-nums;
}
.mini {
  width: var(--icon-btn);
  min-width: var(--icon-btn);
  height: var(--icon-btn);
  min-height: var(--icon-btn);
  padding: 0;
  color: var(--muted);
  background: transparent;
  opacity: 0;
  transition: opacity var(--dur-fast) var(--ease-swift), background var(--dur-fast) var(--ease-swift);
}
.item:hover .mini {
  opacity: 1;
}
.mini:hover:not(:disabled) {
  background: var(--hover);
  color: var(--text);
}
.back {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  color: var(--muted);
  flex-shrink: 0;
}
.title-ellip {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  max-width: 40vw;
}

@media (max-width: 700px) {
  .pl {
    grid-template-columns: minmax(0, 1fr);
  }
  .side {
    border-right: 0;
    padding: 0;
    max-height: none;
  }
  .body {
    padding: 0;
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
  .mini {
    opacity: 1;
  }
  .title-ellip {
    max-width: none;
    flex: 1;
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
