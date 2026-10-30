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
    <aside v-else class="side">
      <div class="side-title">我的歌单</div>
      <div
        v-for="p in lists"
        :key="p.id"
        class="item"
        :class="{ on: active?.id === p.id }"
        @click="open(p)"
      >
        <span>{{ p.isLiked ? '我喜欢的音乐' : p.name }}</span>
        <small class="muted">{{ p.trackCount }}</small>
      </div>
    </aside>
    <div v-if="active" class="body">
      <header class="head">
        <button v-if="narrow" class="back" type="button" @click="backToLists">
          <Icon name="previous" :size="14" /> 歌单
        </button>
        <h2>{{ active.isLiked ? '我喜欢的音乐' : active.name }}</h2>
        <span v-if="fromCache" class="badge muted">缓存</span>
        <button :disabled="!tracks.length" @click="playList(tracks)">播放全部</button>
        <button v-if="canUnsubscribe" :disabled="busy" @click="unsubscribe">取消收藏</button>
        <button v-if="canDelete" :disabled="busy" class="danger" @click="removeOwned">删除歌单</button>
      </header>
      <p v-if="loading && !tracks.length" class="muted">加载中…</p>
      <p v-if="err" class="error">{{ err }}</p>
      <TrackList :tracks="tracks" />
    </div>
  </section>
</template>

<style scoped>
.pl {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
  gap: 16px;
  min-width: 0;
}
.side {
  border-right: 1px solid var(--border);
  padding-right: 8px;
  max-height: calc(100vh - 140px);
  overflow: auto;
}
.side-title {
  display: none;
  font-weight: 700;
  font-size: 22px;
  margin-bottom: 14px;
  letter-spacing: -0.03em;
}
.item {
  display: flex;
  justify-content: space-between;
  gap: 6px;
  padding: 10px 12px;
  border-radius: var(--radius-md);
  cursor: pointer;
  font-weight: 500;
  transition: background var(--dur-fast) var(--ease-swift), color var(--dur-fast) var(--ease-swift),
    transform var(--dur-fast) var(--ease-swift);
}
.item:active {
  transform: scale(0.98);
}
.item span {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  min-width: 0;
}
.item:hover {
  background: var(--fill-secondary);
}
.item.on {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 650;
}
.body {
  min-width: 0;
}
.back {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: none;
  border: 0;
  color: var(--muted);
  padding: 6px 10px;
  border-radius: var(--radius-pill);
}
.back:hover {
  background: var(--hover);
  color: inherit;
}
.badge {
  font-size: 12px;
  padding: 2px 6px;
  border: 1px solid var(--border);
  border-radius: 4px;
}
.danger {
  color: #c62f2f;
}

@media (max-width: 700px) {
  .pl {
    grid-template-columns: minmax(0, 1fr);
    gap: 0;
  }
  .side {
    border-right: 0;
    padding-right: 0;
    max-height: none;
  }
  .side-title {
    display: block;
  }
  .item {
    min-height: 48px;
    padding: 12px 10px;
    font-size: 15px;
  }
  /* 窄屏：一次只显示一侧 */
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
