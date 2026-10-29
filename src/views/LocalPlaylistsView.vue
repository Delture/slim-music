<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { playList } from '../lib/player';
import {
  createLocalPlaylist,
  deleteLocalPlaylist,
  localPl,
  moveLocalTrack,
  refreshLocalPlaylists,
  renameLocalPlaylist,
} from '../lib/localPlaylists';
import type { LocalPlaylist } from '../lib/types';
import TrackList from '../components/TrackList.vue';

const activeId = ref<string | null>(null);
const err = ref('');

const active = computed(
  () => localPl.lists.find((p) => p.id === activeId.value) ?? null,
);

async function ensure() {
  try {
    await refreshLocalPlaylists();
    if (!activeId.value && localPl.lists[0]) activeId.value = localPl.lists[0].id;
  } catch (e) {
    err.value = String(e);
  }
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
    if (activeId.value === p.id) activeId.value = localPl.lists[0]?.id ?? null;
  } catch (e) {
    alert(String(e));
  }
}

async function onReorder(e: { from: number; to: number }) {
  if (!active.value) return;
  try {
    await moveLocalTrack(active.value.id, e.from, e.to);
  } catch (err) {
    alert(String(err));
  }
}

onMounted(ensure);
watch(
  () => localPl.lists.map((p) => p.id).join(','),
  () => {
    if (activeId.value && !localPl.lists.some((p) => p.id === activeId.value)) {
      activeId.value = localPl.lists[0]?.id ?? null;
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
      <p class="hint muted">
        本机列表（类 Walkman），可混入本地与网易云歌曲。从别处点歌只会改「正在播放」队列，不会改这些列表。
      </p>
      <div
        v-for="p in localPl.lists"
        :key="p.id"
        class="item"
        :class="{ on: active?.id === p.id }"
        @click="open(p)"
      >
        <span class="name" :title="p.name" @dblclick.stop="onRename(p)">{{ p.name }}</span>
        <small class="muted">{{ p.tracks.length }}</small>
        <button class="mini" title="重命名" @click.stop="onRename(p)">✎</button>
        <button class="mini" title="删除" @click.stop="onDelete(p)">×</button>
      </div>
      <div v-if="!localPl.lists.length" class="muted empty-side">还没有播放列表，点 ＋ 新建</div>
    </aside>

    <div v-if="active" class="body">
      <header class="head">
        <h2 class="title-ellip">{{ active.name }}</h2>
        <button :disabled="!active.tracks.length" @click="playList(active.tracks)">播放全部</button>
        <button class="ghost" @click="onRename(active)">重命名</button>
        <button class="ghost" @click="onDelete(active)">删除</button>
      </header>
      <p v-if="err" class="error">{{ err }}</p>
      <TrackList
        :tracks="active.tracks"
        :local-playlist-id="active.id"
        :show-reorder="true"
        @reorder="onReorder"
      />
    </div>
    <div v-else class="body muted">选择或新建一个播放列表</div>
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
  margin-bottom: 6px;
}
.link {
  background: none;
  border: 0;
  font-size: 18px;
  cursor: pointer;
  color: var(--accent);
  padding: 0 6px;
}
.hint {
  font-size: 12px;
  margin: 0 0 10px;
  line-height: 1.4;
}
.item {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto auto;
  gap: 4px;
  align-items: center;
  padding: 6px 8px;
  border-radius: 6px;
  cursor: pointer;
}
.item .name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  min-width: 0;
}
.item:hover,
.item.on {
  background: var(--hover);
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
.empty-side {
  padding: 12px 8px;
  font-size: 13px;
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
