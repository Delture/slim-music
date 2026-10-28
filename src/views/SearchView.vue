<script setup lang="ts">
import { ref } from 'vue';
import { api } from '../lib/api';
import type { Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';

const kw = ref('');
const tracks = ref<Track[]>([]);
const err = ref('');
const loading = ref(false);

async function run(more = false) {
  if (!kw.value.trim()) return;
  loading.value = true;
  err.value = '';
  try {
    const res = await api.search(kw.value.trim(), more ? tracks.value.length : 0);
    tracks.value = more ? [...tracks.value, ...res] : res;
  } catch (e) {
    err.value = String(e);
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <section>
    <header class="head">
      <h2>搜索</h2>
      <input v-model="kw" class="grow" placeholder="歌名 / 歌手 / 专辑" @keydown.enter="run()" />
      <button @click="run()">搜索</button>
    </header>
    <p v-if="err" class="error">{{ err }}</p>
    <TrackList :tracks="tracks" />
    <button v-if="tracks.length" :disabled="loading" @click="run(true)">{{ loading ? '加载中…' : '加载更多' }}</button>
  </section>
</template>
