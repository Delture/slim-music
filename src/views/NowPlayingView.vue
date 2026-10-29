<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { current, player, seek } from '../lib/player';
import { closeNowPlaying, isLiked, session, toggleLike } from '../lib/session';
import { setTrayLyricLines, clearTrayLyric } from '../lib/tray';
import { coverSrc } from '../lib/format';
import { loadLocalLyricCached, loadLyricCached, peekLyric, peekLyricForTrack } from '../lib/lyrics';
import type { LyricLine } from '../lib/types';
import Icon from '../components/Icon.vue';
import SourceBadge from '../components/SourceBadge.vue';

const lines = ref<LyricLine[]>([]);
const err = ref('');
const loading = ref(false);
const listEl = ref<HTMLElement | null>(null);

const t = computed(() => current());
const activeIdx = computed(() => {
  const ms = player.currentTime * 1000;
  let idx = -1;
  for (let i = 0; i < lines.value.length; i++) {
    if (lines.value[i].timeMs <= ms) idx = i;
    else break;
  }
  return idx;
});

function applyLyric(payload: { lines: LyricLine[] }) {
  lines.value = payload.lines.filter((l) => l.text.trim());
  err.value = lines.value.length ? '' : '暂无歌词';
  setTrayLyricLines(lines.value);
}

async function loadNeteaseLyric(ncmId: number) {
  err.value = '';
  const warm = peekLyric(`ncm:${ncmId}`);
  if (warm) {
    applyLyric(warm);
    loading.value = false;
    return;
  }
  loading.value = true;
  lines.value = [];
  try {
    applyLyric(await loadLyricCached(ncmId));
  } catch (e) {
    err.value = String(e);
    clearTrayLyric();
  } finally {
    loading.value = false;
  }
}

async function loadLocalLyric(trackId: string, path: string) {
  err.value = '';
  const warm = peekLyric(trackId);
  if (warm) {
    applyLyric(warm);
    loading.value = false;
    return;
  }
  loading.value = true;
  lines.value = [];
  try {
    applyLyric(await loadLocalLyricCached(trackId, path));
  } catch (e) {
    err.value = String(e);
    clearTrayLyric();
  } finally {
    loading.value = false;
  }
}

watch(
  () => t.value?.id,
  () => {
    const track = t.value;
    if (!track) {
      lines.value = [];
      err.value = '无歌曲';
      clearTrayLyric();
      return;
    }
    if (track.source === 'netease' && track.ncmId != null) {
      void loadNeteaseLyric(track.ncmId);
      return;
    }
    if (track.source === 'local' && track.path) {
      // 先 peek，有缓存则立刻显示（同曲重开无闪烁）
      const warm = peekLyricForTrack(track);
      if (warm) {
        applyLyric(warm);
        loading.value = false;
        return;
      }
      void loadLocalLyric(track.id, track.path);
      return;
    }
    lines.value = [];
    err.value = '无歌词';
    clearTrayLyric();
  },
  { immediate: true },
);

watch(activeIdx, async (i) => {
  await nextTick();
  const root = listEl.value;
  const el = root?.querySelector(`[data-i="${i}"]`) as HTMLElement | null;
  if (!root || !el || i < 0) return;
  // 只滚歌词列，避免 scrollIntoView 带动 .content 整页滚动（露出播放条）
  const top = el.offsetTop - root.clientHeight / 2 + el.offsetHeight / 2;
  root.scrollTo({ top: Math.max(0, top), behavior: 'smooth' });
});

async function onLike() {
  const id = t.value?.ncmId;
  if (id == null || !session.profile) return;
  try {
    await toggleLike(id);
  } catch (e) {
    alert(String(e));
  }
}
</script>

<template>
  <div class="np">
    <button class="back" @click="closeNowPlaying"><Icon name="previous" :size="14" /> 返回</button>
    <div class="body">
      <div class="left">
        <img v-if="t?.cover" :src="coverSrc(t.cover, '500y500')" class="cover" />
        <div v-else class="cover ph"><Icon name="music" :size="48" /></div>
        <div class="meta">
          <div class="title">
            {{ t?.title ?? '未在播放' }}
            <SourceBadge v-if="t" :source="t.source" />
          </div>
          <div class="sub">{{ (t?.artists ?? []).join(' / ') }}</div>
          <button
            v-if="t?.source === 'netease' && session.profile"
            class="like"
            :class="{ on: isLiked(t.ncmId) }"
            @click="onLike"
          >
            <Icon :name="isLiked(t?.ncmId) ? 'heart-solid' : 'heart'" :size="18" />
          </button>
        </div>
      </div>
      <div class="right" ref="listEl">
        <p v-if="loading" class="muted">加载歌词…</p>
        <p v-else-if="err" class="muted">{{ err }}</p>
        <div
          v-for="(l, i) in lines"
          :key="i"
          class="line"
          :data-i="i"
          :class="{ on: i === activeIdx }"
          @click="seek(l.timeMs / 1000)"
        >
          <div class="orig">{{ l.text }}</div>
          <div v-if="l.translation" class="trans">{{ l.translation }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.np {
  position: absolute;
  inset: 0;
  z-index: 40;
  background: var(--bg);
  display: flex;
  flex-direction: column;
  padding: 16px 24px 12px;
  overflow: hidden;
  box-sizing: border-box;
}
.back {
  align-self: flex-start;
  display: inline-flex;
  gap: 6px;
  align-items: center;
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  padding: 4px 0;
  margin-bottom: 8px;
}
.back:hover {
  color: inherit;
}
.body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(240px, 38%) 1fr;
  gap: 32px;
}
.left {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  min-height: 0;
  overflow: hidden;
}
.cover {
  width: min(100%, 320px);
  max-width: 100%;
  max-height: min(320px, 45vh);
  aspect-ratio: 1;
  border-radius: 12px;
  object-fit: cover;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25);
  flex-shrink: 1;
}
.ph {
  display: grid;
  place-items: center;
  background: var(--hover);
  color: var(--muted);
}
.meta {
  text-align: center;
  max-width: 320px;
}
.title {
  font-size: 20px;
  font-weight: 600;
}
.sub {
  color: var(--muted);
  margin-top: 6px;
}
.like {
  margin-top: 12px;
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
}
.like.on {
  color: var(--accent);
}
.right {
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 30% 12px;
  mask-image: linear-gradient(transparent, #000 12%, #000 88%, transparent);
  -webkit-mask-image: linear-gradient(transparent, #000 12%, #000 88%, transparent);
}
.line {
  padding: 10px 8px;
  font-size: 18px;
  color: var(--muted);
  cursor: pointer;
  border-radius: 8px;
  transition: color 0.15s, transform 0.15s;
}
.line.on {
  color: inherit;
  font-weight: 700;
  font-size: 22px;
  transform: scale(1.02);
}
.line:hover {
  background: var(--hover);
}
.muted {
  color: var(--muted);
  text-align: center;
}

@media (max-width: 700px) {
  .np { padding: 8px 16px 0; }
  .back { min-height: 44px; font-size: 15px; }
  .body {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto minmax(0, 1fr);
    gap: 12px;
  }
  .left { flex-direction: row; align-items: center; gap: 12px; }
  .left .cover { width: 96px; height: 96px; flex-shrink: 0; }
  .meta { text-align: left; min-width: 0; }
}
</style>
