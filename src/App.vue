<script setup lang="ts">
import { computed, onMounted, ref, type Component } from 'vue';
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

const activeView = computed(() => pages.find((p) => p.key === page.value)!.view);
/** Cache key: page only. Do NOT fold profile id in — that remounts mid-transition and flashes. */
const viewKey = computed(() => page.value);

function go(key: string) {
  if (page.value === key) {
    closeNowPlaying();
    return;
  }
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
  <div
    class="app"
    :class="{
      'np-open': session.nowPlaying,
      'split-main': page === 'playlists' || page === 'localPlaylists',
    }"
  >
    <nav class="nav">
      <a
        v-for="p in pages"
        :key="p.key"
        :class="{ on: page === p.key }"
        @click="go(p.key)"
      >
        <Icon :name="p.icon" :size="17" />
        <span>{{ p.label }}</span>
      </a>
      <div class="user muted">{{ session.profile ? session.profile.nickname : '未登录' }}</div>
    </nav>
    <div class="main-col">
      <main class="content">
        <!-- Transition must see the keyed view vnode. KeepAlive is direct child of
             Transition; key is page-only so out-in never paints a stale sibling. -->
        <Transition v-if="ready" name="page" mode="out-in">
          <KeepAlive>
            <component :is="activeView" :key="viewKey" />
          </KeepAlive>
        </Transition>
      </main>
    </div>
    <PlayerBar class="player" />
    <UpdatePrompt />
    <!-- 全屏沉浸播放页：盖住侧栏 / 底栏 / 内容，三端一致 -->
    <Transition name="np">
      <NowPlayingView v-if="session.nowPlaying" />
    </Transition>
  </div>
</template>

<style>
/* Opacity-only page swap — no translateY (that reads as an extra intermediate frame). */
.page-enter-active,
.page-leave-active {
  transition: opacity var(--dur) var(--ease-swift);
}
.page-enter-from,
.page-leave-to {
  opacity: 0;
}
.page-enter-to,
.page-leave-from {
  opacity: 1;
}
.np-enter-active {
  animation: sm-slide-up var(--dur-slow) var(--ease-enter);
}
.np-leave-active {
  animation: sm-slide-up var(--dur) var(--ease-out) reverse;
}
</style>
