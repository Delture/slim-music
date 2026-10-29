<script setup lang="ts">
import { onMounted, ref, type Component } from 'vue';
import { closeNowPlaying, initSession, session } from './lib/session';
import { bindTrayBridge } from './lib/tray';
import PlayerBar from './components/PlayerBar.vue';
import Icon from './components/Icon.vue';
import DailyView from './views/DailyView.vue';
import SearchView from './views/SearchView.vue';
import PlaylistsView from './views/PlaylistsView.vue';
import LocalPlaylistsView from './views/LocalPlaylistsView.vue';
import LocalView from './views/LocalView.vue';
import SettingsView from './views/SettingsView.vue';
import NowPlayingView from './views/NowPlayingView.vue';

const pages: { key: string; label: string; icon: string; view: Component }[] = [
  { key: 'daily', label: '每日推荐', icon: 'calendar', view: DailyView },
  { key: 'search', label: '搜索', icon: 'search', view: SearchView },
  { key: 'playlists', label: '我的歌单', icon: 'list', view: PlaylistsView },
  { key: 'localPlaylists', label: '播放列表', icon: 'music', view: LocalPlaylistsView },
  { key: 'local', label: '本地音乐', icon: 'folder', view: LocalView },
  { key: 'settings', label: '设置', icon: 'settings', view: SettingsView },
];
const page = ref('daily');
const ready = ref(false);

function go(key: string) {
  page.value = key;
  // 播放页是叠在内容区上的：切侧栏时先退出，再显示目标页
  closeNowPlaying();
}

onMounted(async () => {
  await initSession().catch(console.error);
  await bindTrayBridge().catch(console.error);
  ready.value = true;
});
</script>

<template>
  <div class="app">
    <nav class="nav">
      <a
        v-for="p in pages"
        :key="p.key"
        :class="{ on: page === p.key }"
        @click="go(p.key)"
      >
        <Icon :name="p.icon" :size="15" />
        <span>{{ p.label }}</span>
      </a>
      <div class="user muted">{{ session.profile ? session.profile.nickname : '未登录' }}</div>
    </nav>
    <main class="content">
      <KeepAlive v-if="ready">
        <component
          :is="pages.find((p) => p.key === page)!.view"
          :key="page + (session.profile?.userId ?? 0)"
        />
      </KeepAlive>
      <NowPlayingView v-if="session.nowPlaying" />
    </main>
    <PlayerBar class="player" />
  </div>
</template>
