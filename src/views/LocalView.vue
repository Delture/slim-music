<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { api } from '../lib/api';
import { playList } from '../lib/player';
import { session } from '../lib/session';
import type { Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';
import { isAndroid, queryLocalAudio } from '../lib/native';

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
    if (isAndroid) {
      const list = await queryLocalAudio();
      tracks.value = list.map((a) => ({
        id: 'local:' + a.uri,
        source: 'local',
        title: a.title,
        artists: a.artist ? [a.artist] : [],
        album: a.album,
        durationMs: a.durationMs,
        cover: null,
        path: a.uri,
        ncmId: null,
      }));
    } else tracks.value = await api.scanLocal();
  } catch (e) {
    err.value = String(e);
  } finally {
    loading.value = false;
  }
}
onMounted(() => (isAndroid || session.settings.localFolders.length) && scan());
</script>

<template>
  <section>
    <header class="head">
      <h2 class="head-title">本地音乐</h2>
      <input v-model="filter" class="grow" placeholder="筛选" />
      <div class="head-actions">
        <button class="btn-primary" :disabled="!shown.length" @click="playList(shown)">播放全部</button>
        <button class="btn-secondary" :disabled="loading" @click="scan">{{ loading ? '扫描中…' : isAndroid ? '刷新' : '重新扫描' }}</button>
      </div>
    </header>
    <p v-if="!isAndroid && !session.settings.localFolders.length" class="muted">还没有添加文件夹，去「设置」里添加。</p>
    <p v-if="err" class="error">{{ err }}</p>
    <TrackList :tracks="shown" />
  </section>
</template>
