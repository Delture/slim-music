<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { api } from '../lib/api';
import { playList } from '../lib/player';
import { session } from '../lib/session';
import type { Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';

const tracks = ref<Track[]>([]);
const filter = ref('');
const loading = ref(false);
const err = ref('');

const shown = computed(() => {
  const f = filter.value.trim().toLowerCase();
  if (!f) return tracks.value;
  return tracks.value.filter(t => [t.title, t.album, ...t.artists].some(s => s.toLowerCase().includes(f)));
});

async function scan() {
  loading.value = true;
  err.value = '';
  try {
    tracks.value = await api.scanLocal();
  } catch (e) {
    err.value = String(e);
  } finally {
    loading.value = false;
  }
}
onMounted(() => session.settings.localFolders.length && scan());
</script>

<template>
  <section>
    <header class="head">
      <h2>本地音乐</h2>
      <input v-model="filter" class="grow" placeholder="筛选" />
      <button :disabled="!shown.length" @click="playList(shown)">播放全部</button>
      <button :disabled="loading" @click="scan">{{ loading ? '扫描中…' : '重新扫描' }}</button>
    </header>
    <p v-if="!session.settings.localFolders.length" class="muted">还没有添加文件夹，去「设置」里添加。</p>
    <p v-if="err" class="error">{{ err }}</p>
    <TrackList :tracks="shown" />
  </section>
</template>
