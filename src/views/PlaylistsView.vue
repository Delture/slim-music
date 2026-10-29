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

const canUnsubscribe = computed(
  () => !!active.value && active.value.subscribed && !active.value.isLiked,
);

async function loadLists(preferId?: number) {
  err.value = '';
  try {
    lists.value = await api.myPlaylists();
    const keep =
      (preferId != null && lists.value.find((l) => l.id === preferId)) ||
      lists.value.find((l) => l.id === active.value?.id) ||
      lists.value.find((l) => l.isLiked) ||
      lists.value[0];
    if (keep) await open(keep);
    else {
      active.value = null;
      tracks.value = [];
    }
  } catch (e) {
    err.value = String(e);
  }
}

async function open(p: Playlist) {
  active.value = p;
  tracks.value = [];
  loading.value = true;
  err.value = '';
  try {
    tracks.value = await api.playlistTracks(p.id);
  } catch (e) {
    err.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function unsubscribe() {
  if (!active.value || !canUnsubscribe.value) return;
  if (!confirm(`确定取消收藏「${active.value.name}」？`)) return;
  busy.value = true;
  try {
    await api.playlistSubscribe(active.value.id, false);
    await loadLists();
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
  }
}

onMounted(() => session.profile && loadLists());
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
        <button :disabled="!tracks.length" @click="playList(tracks)">播放全部</button>
        <button v-if="canUnsubscribe" :disabled="busy" @click="unsubscribe">取消收藏</button>
      </header>
      <p v-if="loading" class="muted">加载中…</p>
      <p v-if="err" class="error">{{ err }}</p>
      <TrackList :tracks="tracks" />
    </div>
  </section>
</template>

<style scoped>
.pl {
  display: grid;
  grid-template-columns: 220px 1fr;
  gap: 16px;
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
  padding: 6px 8px;
  border-radius: 6px;
  cursor: pointer;
}
.item span {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.item:hover,
.item.on {
  background: var(--hover);
}
.body {
  min-width: 0;
}
</style>
