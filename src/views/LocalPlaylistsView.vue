<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
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

/** 特殊 id：镜像当前播放队列，不落盘 */
const NOW_PLAYING = '__now_playing__';

const activeId = ref<string>(NOW_PLAYING);
const err = ref('');

const nowPlayingTracks = computed<Track[]>(() => {
  // 按播放顺序展示队列（随机时跟 order）
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

async function ensure() {
  try {
    await refreshLocalPlaylists();
  } catch (e) {
    err.value = String(e);
  }
}

function openNow() {
  activeId.value = NOW_PLAYING;
}
function open(p: LocalPlaylist) {
  activeId.value = p.id;
}

async function onCreate() {
  const name = prompt('新建播放列表名称', '新播放列表');
  if (!name?.trim()) return;
  try {
    const pl = await createLocalPlaylist(name.trim());
    activeId.value = pl.id;
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
    if (activeId.value === p.id) activeId.value = NOW_PLAYING;
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

onMounted(ensure);
watch(
  () => localPl.lists.map((p) => p.id).join(','),
  () => {
    if (
      activeId.value !== NOW_PLAYING &&
      !localPl.lists.some((p) => p.id === activeId.value)
    ) {
      activeId.value = NOW_PLAYING;
    }
  },
);
</script>

<template>
  <section class="pl">
    <aside class="side">
      <div class="side-head">
        <span>播放列表</span>
        <button class="link" title="新建" @click="onCreate">＋</button>
      </div>

      <div class="item" :class="{ on: isNowPlaying }" @click="openNow">
        <span class="name">正在播放</span>
        <small class="muted">{{ nowPlayingTracks.length }}</small>
      </div>

      <div class="sep"></div>

      <div
        v-for="p in localPl.lists"
        :key="p.id"
        class="item"
        :class="{ on: activeId === p.id }"
        @click="open(p)"
      >
        <span class="name" :title="p.name" @dblclick.stop="onRename(p)">{{ p.name }}</span>
        <small class="muted">{{ p.tracks.length }}</small>
        <button class="mini" title="重命名" @click.stop="onRename(p)">✎</button>
        <button class="mini" title="删除" @click.stop="onDelete(p)">×</button>
      </div>
    </aside>

    <div class="body">
      <header class="head">
        <h2 class="title-ellip">{{ activeTitle }}</h2>
        <button
          v-if="activeTracks.length && !isNowPlaying"
          @click="playList(activeTracks)"
        >
          播放全部
        </button>
        <template v-if="activeUser">
          <button class="ghost" @click="onRename(activeUser)">重命名</button>
          <button class="ghost" @click="onDelete(activeUser)">删除</button>
        </template>
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
.pl {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
  gap: 16px;
  min-width: 0;
}
.side {
  border-right: 1px solid var(--border);
  padding-right: 10px;
  max-height: calc(100vh - 140px);
  overflow: auto;
}
.side-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-weight: 600;
  margin-bottom: 10px;
}
.link {
  background: none;
  border: 0;
  font-size: 18px;
  cursor: pointer;
  color: var(--accent);
  padding: 0 6px;
}
.sep {
  height: 1px;
  background: var(--border);
  margin: 8px 0;
}
.item {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto auto;
  gap: 4px;
  align-items: center;
  padding: 8px 10px;
  border-radius: var(--radius);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease-swift), color var(--dur-fast) var(--ease-swift), transform var(--dur-fast) var(--ease-swift);
}
.item:active { transform: scale(0.98); }
.item .name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  min-width: 0;
}
.item:hover {
  background: var(--hover);
}
.item.on {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}
.mini {
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  padding: 2px 4px;
  opacity: 0;
}
.item:hover .mini {
  opacity: 1;
}
.body {
  min-width: 0;
}
.title-ellip {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  max-width: 40vw;
}
.ghost {
  background: transparent;
}
</style>
