<script setup lang="ts">
import { onActivated, onMounted, ref } from 'vue';
import { api } from '../lib/api';
import { playList } from '../lib/player';
import type { Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';
import TrackSkeleton from '../components/TrackSkeleton.vue';

const tracks = ref<Track[]>([]);
const err = ref('');
const loading = ref(false);

async function load() {
  if (loading.value) return;
  loading.value = true;
  err.value = '';
  try {
    tracks.value = await api.daily();
  } catch (e) {
    err.value = String(e) + '（每日推荐需要登录）';
  } finally {
    loading.value = false;
  }
}
onMounted(load);
onActivated(() => {
  if (!tracks.value.length && !loading.value) void load();
});
</script>

<template>
  <section>
    <header class="head">
      <h2 class="head-title">每日推荐</h2>
      <div class="head-actions">
        <button class="btn-primary" :disabled="!tracks.length" @click="playList(tracks)">播放全部</button>
        <button class="btn-secondary" :disabled="loading" @click="load">{{ loading ? '刷新中…' : '刷新' }}</button>
      </div>
    </header>
    <p v-if="err" class="error">{{ err }}</p>
    <TrackSkeleton v-if="loading && !tracks.length" />
    <TrackList v-else :tracks="tracks" />
  </section>
</template>
