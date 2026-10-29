<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { api } from '../lib/api';
import { current, player, seek } from '../lib/player';
import { closeNowPlaying, isLiked, session, toggleLike } from '../lib/session';
import { setTrayLyricLines, clearTrayLyric } from '../lib/tray';
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

async function loadLyric(id: number) {
  loading.value = true;
  err.value = '';
  lines.value = [];
  try {
    const payload = await api.lyric(id);
    lines.value = payload.lines.filter((l) => l.text.trim());
    if (!lines.value.length) err.value = '暂无歌词';
    setTrayLyricLines(lines.value);
  } catch (e) {
    err.value = String(e);
    clearTrayLyric();
  } finally {
    loading.value = false;
  }
}

watch(
  () => t.value?.id,
  (id) => {
    const track = t.value;
    if (!track || track.source !== 'netease' || track.ncmId == null) {
      lines.value = [];
      err.value = track?.source === 'local' ? '本地歌曲暂不支持在线歌词' : '无歌曲';
      clearTrayLyric();
      return;
    }
    void loadLyric(track.ncmId);
  },
  { immediate: true },
);

watch(activeIdx, async (i) => {
  await nextTick();
  const el = listEl.value?.querySelector(`[data-i="${i}"]`) as HTMLElement | null;
  el?.scrollIntoView({ block: 'center', behavior: 'smooth' });
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
        <img v-if="t?.cover" :src="t.cover.replace('200y200', '500y500')" class="cover" />
        <div v-else class="cover ph"><Icon name="music" :size="48" /></div>
        <div class="meta">
          <div class="title">
            {{ t?.title ?? '未在播放' }}
            <SourceBadge v-if="t" :source="t.source" />
          </div>
          <div class="sub">{{ t?.artists.join(' / ') }}</div>
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
          {{ l.text }}
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.np {
  position: absolute;
  inset: 0;
  z-index: 20;
  background: var(--bg);
  display: flex;
  flex-direction: column;
  padding: 16px 24px 12px;
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
}
.cover {
  width: min(100%, 320px);
  aspect-ratio: 1;
  border-radius: 12px;
  object-fit: cover;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25);
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
  overflow: auto;
  padding: 20vh 12px;
  mask-image: linear-gradient(transparent, #000 15%, #000 85%, transparent);
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
</style>
