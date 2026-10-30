<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { api } from '../lib/api';
import { playList } from '../lib/player';
import { session } from '../lib/session';
import type { Playlist, Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';
import Icon from '../components/Icon.vue';

const lists = ref<Playlist[]>([]);
const active = ref<Playlist | null>(null);
const tracks = ref<Track[]>([]);
const err = ref('');
const loading = ref(false);
const busy = ref(false);
const fromCache = ref(false);
/** 窄屏：列表 ↔ 详情 叠层导航 */
const mobileDetail = ref(false);
const narrow = ref(false);
let mql: MediaQueryList | null = null;

const canUnsubscribe = computed(
  () => !!active.value && active.value.subscribed && !active.value.isLiked,
);
const canDelete = computed(
  () => !!active.value && active.value.owned && !active.value.isLiked,
);

function syncNarrow() {
  narrow.value = !!mql?.matches;
  if (!narrow.value) mobileDetail.value = false;
}

async function loadLists(refresh = false) {
  err.value = '';
  try {
    const hit = await api.myPlaylists(refresh);
    lists.value = hit.data;
    fromCache.value = hit.fromCache;
    const keep =
      lists.value.find((l) => l.id === active.value?.id) ||
      lists.value.find((l) => l.isLiked) ||
      lists.value[0];
    if (keep) {
      // 窄屏只预选，不自动钻进详情，避免侧栏占满宽度
      await open(keep, false, !narrow.value);
    } else {
      active.value = null;
      tracks.value = [];
      mobileDetail.value = false;
    }
    if (!refresh && hit.fromCache) {
      void api.myPlaylists(true).then((h) => {
        lists.value = h.data;
        fromCache.value = false;
      });
    }
  } catch (e) {
    err.value = String(e);
  }
}

async function open(p: Playlist, doBackgroundRefresh = true, enterDetail = true) {
  active.value = p;
  if (enterDetail && narrow.value) mobileDetail.value = true;
  loading.value = true;
  err.value = '';
  try {
    const hit = await api.playlistTracks(p.id, false);
    if (active.value?.id !== p.id) return;
    tracks.value = hit.data;
    fromCache.value = hit.fromCache;
    loading.value = false;
    if (doBackgroundRefresh && (hit.fromCache || hit.ageSecs > 0)) {
      void api.playlistTracks(p.id, true).then((h) => {
        if (active.value?.id === p.id) {
          tracks.value = h.data;
          fromCache.value = false;
        }
      });
    }
  } catch (e) {
    if (active.value?.id === p.id) {
      err.value = String(e);
      loading.value = false;
    }
  }
}

function backToLists() {
  mobileDetail.value = false;
}

async function unsubscribe() {
  if (!active.value || !canUnsubscribe.value) return;
  if (!confirm(`确定取消收藏「${active.value.name}」？`)) return;
  busy.value = true;
  try {
    await api.playlistSubscribe(active.value.id, false);
    mobileDetail.value = false;
    await loadLists(true);
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
  }
}

async function removeOwned() {
  if (!active.value || !canDelete.value) return;
  if (
    !confirm(
      `确定删除网易云歌单「${active.value.name}」？\n此操作不可恢复，歌单内歌曲不会从云盘删除。`,
    )
  ) {
    return;
  }
  busy.value = true;
  try {
    await api.playlistDelete(active.value.id);
    active.value = null;
    mobileDetail.value = false;
    await loadLists(true);
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  mql = window.matchMedia('(max-width: 700px)');
  syncNarrow();
  mql.addEventListener('change', syncNarrow);
  if (session.profile) void loadLists(false);
});
onUnmounted(() => {
  mql?.removeEventListener('change', syncNarrow);
});
</script>

<template>
  <section class="pl" :class="{ 'mobile-detail': mobileDetail }">
    <p v-if="!session.profile" class="muted">请先在「设置」中扫码登录网易云。</p>
    <aside v-else class="side panel">
      <div class="side-title">我的歌单</div>
      <div
        v-for="p in lists"
        :key="p.id"
        class="list-row item"
        :class="{ on: active?.id === p.id }"
        @click="open(p)"
      >
        <span class="name">{{ p.isLiked ? '我喜欢的音乐' : p.name }}</span>
        <small class="muted count">{{ p.trackCount }}</small>
      </div>
    </aside>
    <div v-if="active" class="body panel">
      <header class="head">
        <button v-if="narrow" class="back btn-ghost" type="button" @click="backToLists">
          <Icon name="chevron-left" :size="14" /> 歌单
        </button>
        <h2 class="head-title">{{ active.isLiked ? '我喜欢的音乐' : active.name }}</h2>
        <div class="head-actions">
          <span v-if="fromCache" class="sm-badge muted-chip">缓存</span>
          <button class="btn-primary" :disabled="!tracks.length" @click="playList(tracks)">播放全部</button>
          <button v-if="canUnsubscribe" class="btn-ghost" :disabled="busy" @click="unsubscribe">取消收藏</button>
          <button v-if="canDelete" class="btn-danger" :disabled="busy" @click="removeOwned">删除歌单</button>
        </div>
      </header>
      <p v-if="loading && !tracks.length" class="muted">加载中…</p>
      <p v-if="err" class="error">{{ err }}</p>
      <TrackList :tracks="tracks" />
    </div>
  </section>
</template>

<style scoped>
/* Co-visible panes share ONE --panel-* token (same fill/rim). Divider only. */
.pl {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  gap: 0;
  min-width: 0;
  background: var(--panel-bg);
  border-radius: var(--panel-radius);
}
.panel {
  background: var(--panel-bg);
  border: 0;
  box-shadow: var(--panel-shadow);
  min-width: 0;
}
.side {
  border-right: var(--panel-divider);
  padding: 0 var(--space-2) 0 0;
  max-height: calc(100vh - 140px);
  overflow: auto;
}
.body {
  padding: 0 0 0 var(--space-4);
  min-width: 0;
}
.side-title {
  display: none;
  font-family: var(--font-display);
  font-weight: 500;
  font-size: 24px;
  margin-bottom: var(--space-3);
  letter-spacing: -0.02em;
}
.item {
  justify-content: space-between;
}
.item .name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  min-width: 0;
  flex: 1;
}
.item .count {
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
}
.back {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  color: var(--muted);
  flex-shrink: 0;
}

@media (max-width: 700px) {
  .pl {
    grid-template-columns: minmax(0, 1fr);
    gap: 0;
  }
  .side {
    border-right: 0;
    padding: 0;
    max-height: none;
  }
  .body {
    padding: 0;
  }
  .side-title {
    display: block;
  }
  .item {
    min-height: 48px;
    font-size: 15px;
  }
  .pl:not(.mobile-detail) .body {
    display: none;
  }
  .pl.mobile-detail .side {
    display: none;
  }
  .pl.mobile-detail .body {
    animation: sm-fade-in var(--dur) var(--ease-enter);
  }
}
</style>
