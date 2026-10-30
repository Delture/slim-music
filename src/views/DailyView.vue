<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { api } from '../lib/api';
import { playList } from '../lib/player';
import type { Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';

const tracks = ref<Track[]>([]);
const err = ref('');
const loading = ref(false);

async function load() {
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
</script>

<template>
  <section>
    <header class="head">
      <h2>每日推荐</h2>
      <button class="btn-primary" :disabled="!tracks.length" @click="playList(tracks)">播放全部</button>
      <button class="btn-secondary" @click="load">刷新</button>
    </header>
    <p v-if="loading" class="muted">加载中…</p>
    <p v-if="err" class="error">{{ err }}</p>
    <TrackList :tracks="tracks" />
  </section>
</template>
