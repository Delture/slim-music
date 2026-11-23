<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, type Component } from 'vue';
import { closeNowPlaying, initSession, session } from './lib/session';
import { dialogState } from './lib/dialog';
import { togglePlay } from './lib/player';
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
import AppDialog from './components/AppDialog.vue';
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
/** Drives split chrome; synced around Transition hooks so leave+gap keep the
 *  leaving page's chrome (solid panel), matching tracks-swap's stable parent. */
const chromeSplit = ref(false);

const activeView = computed(() => pages.find((p) => p.key === page.value)!.view);
/** Cache key: page only. Do NOT fold profile id in — that remounts mid-transition and flashes. */
const viewKey = computed(() => page.value);

function isSplitKey(key: string) {
  return key === 'playlists' || key === 'localPlaylists' || key === 'local';
}

function go(key: string) {
  if (page.value === key) {
    closeNowPlaying();
    return;
  }
  // Lock chrome to the page we are leaving for the whole leave + empty gap.
  chromeSplit.value = isSplitKey(page.value);
  page.value = key;
  closeNowPlaying();
}

/** After leave empties the stage, flip chrome to the destination before enter. */
function onPageAfterLeave() {
  chromeSplit.value = isSplitKey(page.value);
}

function isEditableTarget(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  if (el.isContentEditable) return true;
  const tag = el.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || tag === 'BUTTON';
}

function onGlobalKey(e: KeyboardEvent) {
  if (e.code !== 'Space' || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
  if (e.isComposing || dialogState.open || isEditableTarget(e.target)) return;
  e.preventDefault();
  togglePlay();
}

onMounted(async () => {
  window.addEventListener('keydown', onGlobalKey);
  await initSession().catch(console.error);
  await bindTrayBridge().catch(console.error);
  chromeSplit.value = isSplitKey(page.value);
  ready.value = true;
  setTimeout(() => checkUpdate(true), 3000);
});
onUnmounted(() => {
  window.removeEventListener('keydown', onGlobalKey);
});
</script>

<template>
  <div
    class="app"
    :class="{
      'np-open': session.nowPlaying,
      'split-main': chromeSplit,
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
        <!-- Same motion as playlist tracks-swap (fade + slight X slide, out-in).
             Chrome stays on the leaving page through leave+gap so the empty frame
             sits on a solid panel — no window-bg flash (e.g. 搜索→我的歌单). -->
        <Transition
          v-if="ready"
          name="page-swap"
          mode="out-in"
          @after-leave="onPageAfterLeave"
        >
          <KeepAlive>
            <component :is="activeView" :key="viewKey" />
          </KeepAlive>
        </Transition>
      </main>
    </div>
    <PlayerBar class="player" />
    <UpdatePrompt />
    <AppDialog />
    <!-- 全屏沉浸播放页：盖住侧栏 / 底栏 / 内容，三端一致 -->
    <Transition name="np">
      <NowPlayingView v-if="session.nowPlaying" />
    </Transition>
  </div>
</template>

<style>
/* 单栏页切换：轻淡入 + 横向微移（out-in）。 */
.page-swap-enter-active {
  transition:
    opacity var(--dur) var(--ease-enter),
    transform var(--dur) var(--ease-enter);
}
.page-swap-leave-active {
  transition:
    opacity 150ms var(--ease-out),
    transform 150ms var(--ease-out);
}
.page-swap-enter-from {
  opacity: 0;
  transform: translateX(10px);
}
.page-swap-leave-to {
  opacity: 0;
  transform: translateX(-6px);
}

/* 双栏浮动面板页（我的歌单 / 播放列表 / 本地音乐）：中栏先落位，右栏错峰 65ms 滑入。
   外层容器只承载总过渡时长，避免两个独立面板焊成一整块同步出现。 */
.page-swap-enter-active:is(.pl, .lv) {
  transition: opacity 290ms linear;
}
.page-swap-enter-from:is(.pl, .lv) {
  opacity: 0.999;
  transform: none;
}
.page-swap-enter-active:is(.pl, .lv) > .side.float-panel,
.page-swap-enter-active:is(.pl, .lv) > .login-hint.float-panel {
  transition:
    opacity 210ms var(--ease-enter) 0ms,
    transform 210ms var(--ease-enter) 0ms;
}
.page-swap-enter-active:is(.pl, .lv) > .body.float-panel {
  transition:
    opacity 220ms var(--ease-enter) 65ms,
    transform 220ms var(--ease-enter) 65ms;
}
.page-swap-enter-from:is(.pl, .lv) > .side.float-panel,
.page-swap-enter-from:is(.pl, .lv) > .login-hint.float-panel {
  opacity: 0;
  transform: translateX(8px) scale(0.994);
}
.page-swap-enter-from:is(.pl, .lv) > .body.float-panel {
  opacity: 0;
  transform: translateX(14px) scale(0.994);
}

/* 双栏退场：右栏先轻退，中栏晚 25ms 收起 */
.page-swap-leave-active:is(.pl, .lv) {
  transition: opacity 170ms linear;
}
.page-swap-leave-to:is(.pl, .lv) {
  opacity: 0.999;
  transform: none;
}
.page-swap-leave-active:is(.pl, .lv) > .body.float-panel {
  transition:
    opacity 135ms var(--ease-out) 0ms,
    transform 135ms var(--ease-out) 0ms;
}
.page-swap-leave-active:is(.pl, .lv) > .side.float-panel,
.page-swap-leave-active:is(.pl, .lv) > .login-hint.float-panel {
  transition:
    opacity 135ms var(--ease-out) 25ms,
    transform 135ms var(--ease-out) 25ms;
}
.page-swap-leave-to:is(.pl, .lv) > .body.float-panel {
  opacity: 0;
  transform: translateX(-8px);
}
.page-swap-leave-to:is(.pl, .lv) > .side.float-panel,
.page-swap-leave-to:is(.pl, .lv) > .login-hint.float-panel {
  opacity: 0;
  transform: translateX(-5px);
}

@media (max-width: 700px) {
  .page-swap-enter-active:is(.pl, .lv) > .body.float-panel,
  .page-swap-leave-active:is(.pl, .lv) > .side.float-panel {
    transition-delay: 0ms;
  }
}

.np-enter-active {
  animation: sm-slide-up var(--dur-slow) var(--ease-enter);
}
.np-leave-active {
  animation: sm-slide-up var(--dur) var(--ease-out) reverse;
}
</style>
