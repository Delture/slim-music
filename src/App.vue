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
import UpdatePrompt from './components/UpdatePrompt.vue';
import { checkUpdate } from './lib/updater';

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
  closeNowPlaying();
}

onMounted(async () => {
  await initSession().catch(console.error);
  await bindTrayBridge().catch(console.error);
  ready.value = true;
  setTimeout(() => checkUpdate(true), 3000);
});
</script>

<template>
  <div class="app" :class="{ 'np-open': session.nowPlaying }">
    <nav class="nav">
      <a
        v-for="p in pages"
        :key="p.key"
        :class="{ on: page === p.key && !session.nowPlaying }"
        @click="go(p.key)"
      >
        <Icon :name="p.icon" :size="15" />
        <span>{{ p.label }}</span>
      </a>
      <div class="user muted">{{ session.profile ? session.profile.nickname : '未登录' }}</div>
    </nav>
    <div class="main-col">
      <main class="content" :class="{ locked: session.nowPlaying }">
        <Transition name="page" mode="out-in">
          <KeepAlive v-if="ready">
            <component
              :is="pages.find((p) => p.key === page)!.view"
              :key="page + '-' + (session.profile?.userId ?? 0)"
            />
          </KeepAlive>
        </Transition>
      </main>
      <Transition name="np">
        <NowPlayingView v-if="session.nowPlaying" />
      </Transition>
    </div>
    <PlayerBar class="player" />
    <UpdatePrompt />
  </div>
</template>

<style>
.page-enter-active {
  animation: sm-fade-in var(--dur-slow) var(--ease-enter);
}
.page-leave-active {
  animation: sm-fade-in var(--dur-fast) var(--ease-out) reverse;
}
.np-enter-active {
  animation: sm-slide-up var(--dur-slow) var(--ease-enter);
}
.np-leave-active {
  animation: sm-slide-up var(--dur) var(--ease-out) reverse;
}
</style>
