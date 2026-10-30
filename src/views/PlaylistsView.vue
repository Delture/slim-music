<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { api } from '../lib/api';
import { playList } from '../lib/player';
import { session } from '../lib/session';
import type { Playlist, Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';

const lists = ref<Playlist[]>([]);
const active = ref<Playlist | null>(null);
const tracks = ref<Track[]>([]);
const err = ref('');
const loading = ref(false);
const busy = ref(false);
const fromCache = ref(false);

const canUnsubscribe = computed(
  () => !!active.value && active.value.subscribed && !active.value.isLiked,
);
const canDelete = computed(
  () => !!active.value && active.value.owned && !active.value.isLiked,
);

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
    if (keep) await open(keep, false);
    else {
      active.value = null;
      tracks.value = [];
    }
    // 后台刷新歌单列表
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

async function open(p: Playlist, doBackgroundRefresh = true) {
  active.value = p;
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

async function unsubscribe() {
  if (!active.value || !canUnsubscribe.value) return;
  if (!confirm(`确定取消收藏「${active.value.name}」？`)) return;
  busy.value = true;
  try {
    await api.playlistSubscribe(active.value.id, false);
    await loadLists(true);
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
  }
}

async function removeOwned() {
  if (!active.value || !canDelete.value) return;
  if (
    !confirm(
      `确定删除网易云歌单「${active.value.name}」？\n此操作不可恢复，歌单内歌曲不会从云盘删除。`,
    )
  ) {
    return;
  }
  busy.value = true;
  try {
    await api.playlistDelete(active.value.id);
    active.value = null;
    await loadLists(true);
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
  }
}

onMounted(() => session.profile && loadLists(false));
</script>

<template>
  <section class="pl">
    <p v-if="!session.profile" class="muted">请先在「设置」中扫码登录网易云。</p>
    <aside v-else class="side">
      <div
        v-for="p in lists"
        :key="p.id"
        class="item"
        :class="{ on: active?.id === p.id }"
        @click="open(p)"
      >
        <span>{{ p.isLiked ? '我喜欢的音乐' : p.name }}</span>
        <small class="muted">{{ p.trackCount }}</small>
      </div>
    </aside>
    <div v-if="active" class="body">
      <header class="head">
        <h2>{{ active.isLiked ? '我喜欢的音乐' : active.name }}</h2>
        <span v-if="fromCache" class="badge muted">缓存</span>
        <button :disabled="!tracks.length" @click="playList(tracks)">播放全部</button>
        <button v-if="canUnsubscribe" :disabled="busy" @click="unsubscribe">取消收藏</button>
        <button v-if="canDelete" :disabled="busy" class="danger" @click="removeOwned">删除歌单</button>
      </header>
      <p v-if="loading && !tracks.length" class="muted">加载中…</p>
      <p v-if="err" class="error">{{ err }}</p>
      <TrackList :tracks="tracks" />
    </div>
  </section>
</template>

<style scoped>
.pl {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  gap: 16px;
  min-width: 0;
}
.side {
  border-right: 1px solid var(--border);
  padding-right: 8px;
  max-height: calc(100vh - 140px);
  overflow: auto;
}
.item {
  display: flex;
  justify-content: space-between;
  gap: 6px;
  padding: 8px 10px;
  border-radius: var(--radius);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease-swift), color var(--dur-fast) var(--ease-swift), transform var(--dur-fast) var(--ease-swift);
}
.item:active { transform: scale(0.98); }
.item span {
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
.body {
  min-width: 0;
}
.badge {
  font-size: 12px;
  padding: 2px 6px;
  border: 1px solid var(--border);
  border-radius: 4px;
}
.danger {
  color: #c62f2f;
}
</style>
