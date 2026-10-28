<script setup lang="ts">
import { onMounted, ref, type Component } from 'vue';
import { initSession, session } from './lib/session';
import PlayerBar from './components/PlayerBar.vue';
import DailyView from './views/DailyView.vue';
import SearchView from './views/SearchView.vue';
import PlaylistsView from './views/PlaylistsView.vue';
import LocalView from './views/LocalView.vue';
import SettingsView from './views/SettingsView.vue';

// 页面注册表：增删页面只改这里
const pages: { key: string; label: string; view: Component }[] = [
  { key: 'daily', label: '每日推荐', view: DailyView },
  { key: 'search', label: '搜索', view: SearchView },
  { key: 'playlists', label: '我的歌单', view: PlaylistsView },
  { key: 'local', label: '本地音乐', view: LocalView },
  { key: 'settings', label: '设置', view: SettingsView },
];
const page = ref('daily');
const ready = ref(false);
onMounted(async () => {
  await initSession().catch(console.error);
  ready.value = true;
});
</script>

<template>
  <div class="app">
    <nav class="nav">
      <div class="logo">SlimMusic</div>
      <a v-for="p in pages" :key="p.key" :class="{ on: page === p.key }" @click="page = p.key">{{ p.label }}</a>
      <div class="user muted">{{ session.profile ? session.profile.nickname : '未登录' }}</div>
    </nav>
    <main class="content">
      <KeepAlive v-if="ready">
        <component :is="pages.find(p => p.key === page)!.view" :key="page + (session.profile?.userId ?? 0)" />
      </KeepAlive>
    </main>
    <PlayerBar class="player" />
  </div>
</template>
