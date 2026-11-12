<script setup lang="ts">
import { computed, onActivated, onMounted, ref } from 'vue';
import { api } from '../lib/api';
import {
  ensureLocalLibrary,
  localLibraryError,
  localLibraryReady,
  localLibraryTracks,
  refreshLocalLibrary,
} from '../lib/localLibrary';
import { playList } from '../lib/player';
import { filterTracks } from '../lib/trackFilter';
import type { Track } from '../lib/types';
import TrackList from '../components/TrackList.vue';
import Icon from '../components/Icon.vue';

const NCM_PAGE = 50;
const DEBOUNCE_MS = 350;

const kw = ref('');
const localLoading = ref(false);
const ncmTracks = ref<Track[]>([]);
const ncmErr = ref('');
const ncmLoading = ref(false);
const ncmHasMore = ref(false);
const ncmQuery = ref('');

let ncmSeq = 0;
let debounceTimer = 0;

const q = computed(() => kw.value.trim());

const localTracks = computed(() => {
  if (!q.value) return [];
  return filterTracks(localLibraryTracks.value, q.value);
});

async function bootLocal() {
  if (localLibraryReady.value) return;
  localLoading.value = true;
  try {
    await ensureLocalLibrary();
  } finally {
    localLoading.value = false;
  }
}

function clearNcm() {
  ncmSeq++;
  ncmTracks.value = [];
  ncmErr.value = '';
  ncmLoading.value = false;
  ncmHasMore.value = false;
  ncmQuery.value = '';
}

function onQueryInput() {
  if (debounceTimer) window.clearTimeout(debounceTimer);
  if (!q.value) {
    clearNcm();
    return;
  }
  // 查询变了就先收起上一轮网易云结果，避免串到新关键词下面
  if (q.value !== ncmQuery.value) {
    ncmTracks.value = [];
    ncmErr.value = '';
    ncmHasMore.value = false;
    ncmLoading.value = true; // 防抖期间先显示加载，避免闪「没有匹配」
  }
  debounceTimer = window.setTimeout(() => {
    debounceTimer = 0;
    void searchNcm(false);
  }, DEBOUNCE_MS);
}

function run() {
  if (debounceTimer) {
    window.clearTimeout(debounceTimer);
    debounceTimer = 0;
  }
  if (!q.value) {
    clearNcm();
    return;
  }
  void searchNcm(false);
}

function clearQuery() {
  kw.value = '';
  if (debounceTimer) {
    window.clearTimeout(debounceTimer);
    debounceTimer = 0;
  }
  clearNcm();
}

async function searchNcm(more: boolean) {
  const keywords = q.value;
  if (!keywords) return;
  if (more && (ncmLoading.value || !ncmHasMore.value || ncmQuery.value !== keywords)) return;
  const seq = ++ncmSeq;
  ncmLoading.value = true;
  if (!more) ncmErr.value = '';
  try {
    const offset = more ? ncmTracks.value.length : 0;
    const res = await api.search(keywords, offset);
    if (seq !== ncmSeq) return;
    ncmTracks.value = more ? [...ncmTracks.value, ...res] : res;
    ncmHasMore.value = res.length >= NCM_PAGE;
    ncmQuery.value = keywords;
  } catch (e) {
    if (seq !== ncmSeq) return;
    if (!more) {
      ncmTracks.value = [];
      ncmHasMore.value = false;
    }
    ncmQuery.value = keywords;
    const raw = String(e ?? '').trim();
    ncmErr.value = raw && raw !== 'Error'
      ? `网易云搜索失败：${raw}`
      : '网易云搜索失败，请稍后重试';
  } finally {
    if (seq === ncmSeq) ncmLoading.value = false;
  }
}

function playLocal(i = 0) {
  if (localTracks.value.length) playList(localTracks.value, i);
}
function playNcm(i = 0) {
  if (ncmTracks.value.length) playList(ncmTracks.value, i);
}

const showResults = computed(() => !!q.value);
const localEmptyHint = computed(() => {
  if (!q.value) return '';
  if (!localLibraryReady.value || localLoading.value) return '正在读取本机曲库…';
  if (localLibraryError.value) return localLibraryError.value;
  if (!localLibraryTracks.value.length) return '本机曲库还是空的';
  return `没有匹配「${q.value}」的本机歌曲`;
});

onMounted(() => {
  void bootLocal();
});
onActivated(() => {
  if (localLibraryReady.value) void refreshLocalLibrary();
});
</script>

<template>
  <section class="sv">
    <header class="head">
      <h2 class="head-title">搜索</h2>
      <div class="q">
        <input
          v-model="kw"
          class="q-inp"
          type="search"
          placeholder="歌名 / 歌手 / 专辑"
          autocomplete="off"
          spellcheck="false"
          enterkeyhint="search"
          @input="onQueryInput"
          @keydown.enter.prevent="run()"
        />
        <button
          v-if="kw"
          type="button"
          class="q-clr"
          title="清除"
          aria-label="清除搜索"
          @click="clearQuery"
        >
          <Icon name="x" :size="12" />
        </button>
      </div>
      <div class="head-actions">
        <button class="btn-primary" :disabled="!q" @click="run()">搜索</button>
      </div>
    </header>

    <p v-if="!showResults" class="muted empty-hint">输入歌名、歌手或专辑，同时搜索本机和网易云</p>

    <template v-else>
      <section class="sec" aria-labelledby="sec-local">
        <header class="sec-head">
          <h3 id="sec-local" class="sec-title">本机</h3>
          <small v-if="localTracks.length" class="muted sec-count">{{ localTracks.length }}</small>
          <div class="sec-actions">
            <button
              v-if="localTracks.length"
              type="button"
              class="btn-ghost"
              @click="playLocal(0)"
            >播放全部</button>
          </div>
        </header>
        <TrackList v-if="localTracks.length" :tracks="localTracks" />
        <p v-else class="muted empty-hint">{{ localEmptyHint }}</p>
      </section>

      <section class="sec" aria-labelledby="sec-ncm">
        <header class="sec-head">
          <h3 id="sec-ncm" class="sec-title">网易云</h3>
          <small v-if="ncmTracks.length" class="muted sec-count">{{ ncmTracks.length }}</small>
          <span v-if="ncmLoading && !ncmTracks.length" class="muted sec-status">加载中…</span>
          <div class="sec-actions">
            <button
              v-if="ncmTracks.length"
              type="button"
              class="btn-ghost"
              @click="playNcm(0)"
            >播放全部</button>
          </div>
        </header>
        <p v-if="ncmErr" class="error">{{ ncmErr }}</p>
        <TrackList v-if="ncmTracks.length" :tracks="ncmTracks" />
        <p
          v-else-if="!ncmLoading && !ncmErr"
          class="muted empty-hint"
        >没有匹配「{{ q }}」的网易云歌曲</p>
        <button
          v-if="ncmTracks.length && ncmHasMore"
          class="more"
          :disabled="ncmLoading"
          @click="searchNcm(true)"
        >{{ ncmLoading ? '加载中…' : '加载更多' }}</button>
      </section>
    </template>
  </section>
</template>

<style scoped>
.sv {
  min-width: 0;
}
.q {
  flex: 1;
  min-width: 120px;
  display: flex;
  align-items: center;
  position: relative;
}
.q-inp {
  width: 100%;
  padding-right: 36px;
}
.q-inp::-webkit-search-cancel-button,
.q-inp::-webkit-search-decoration {
  -webkit-appearance: none;
  appearance: none;
}
.q-clr {
  position: absolute;
  right: 6px;
  width: 22px;
  height: 22px;
  min-width: 22px;
  min-height: 22px;
  padding: 0;
  border: 0;
  border-radius: var(--radius);
  background: var(--hover);
  color: var(--muted);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  line-height: 0;
}
.q-clr:hover {
  background: var(--fill-secondary);
  color: var(--text);
}
.sec {
  margin-top: var(--space-5);
  min-width: 0;
}
.sec-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-2);
  min-height: var(--btn-h);
}
.sec-title {
  margin: 0;
  font-family: var(--font-display);
  font-weight: 500;
  font-size: 17px;
  letter-spacing: -0.015em;
  line-height: var(--head-title-lh);
}
.sec-count {
  font-variant-numeric: tabular-nums;
  font-size: 13px;
}
.sec-status {
  font-size: 13px;
}
.sec-actions {
  margin-left: auto;
  display: inline-flex;
  gap: var(--space-2);
}
.empty-hint {
  padding: var(--empty-pad);
}
.more {
  margin-top: var(--space-3);
}
</style>
