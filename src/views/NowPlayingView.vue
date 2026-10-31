<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import {
  current,
  cycleRepeat,
  next,
  player,
  prev,
  seek,
  togglePlay,
  toggleShuffle,
} from '../lib/player';
import { closeNowPlaying, isLiked, session, toggleLike } from '../lib/session';
import { setTrayLyricLines, clearTrayLyric } from '../lib/tray';
import { coverSrc, fmtTime } from '../lib/format';
import { loadLocalLyricCached, loadLyricCached, peekLyric, peekLyricForTrack } from '../lib/lyrics';
import type { LyricLine } from '../lib/types';
import Icon from '../components/Icon.vue';
import SourceBadge from '../components/SourceBadge.vue';

const lines = ref<LyricLine[]>([]);
const err = ref('');
const loading = ref(false);
const listEl = ref<HTMLElement | null>(null);
const rootEl = ref<HTMLElement | null>(null);

const t = computed(() => current());
const repeatLabel = computed(
  () => ({ off: '不循环', all: '列表循环', one: '单曲循环' })[player.repeat],
);
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

/* ---- seek UI (rAF, same approach as PlayerBar) ---- */
const trackEl = ref<HTMLElement | null>(null);
const fillEl = ref<HTMLElement | null>(null);
const thumbEl = ref<HTMLElement | null>(null);
const timeCurEl = ref<HTMLElement | null>(null);
const timeDurEl = ref<HTMLElement | null>(null);
let seeking = false;
let seekRatio = 0;
let raf = 0;
let lastCur = '';
let lastDur = '';

function ratioNow() {
  if (seeking) return seekRatio;
  const d = player.duration;
  if (!d || !isFinite(d) || d <= 0) return 0;
  return Math.min(1, Math.max(0, player.currentTime / d));
}
function paint(r: number) {
  if (fillEl.value) fillEl.value.style.transform = `scaleX(${r})`;
  if (thumbEl.value) thumbEl.value.style.left = `${r * 100}%`;
  const cur = seeking ? r * (player.duration || 0) : player.currentTime;
  const curT = fmtTime(cur);
  const durT = fmtTime(player.duration);
  if (timeCurEl.value && curT !== lastCur) {
    timeCurEl.value.textContent = curT;
    lastCur = curT;
  }
  if (timeDurEl.value && durT !== lastDur) {
    timeDurEl.value.textContent = durT;
    lastDur = durT;
  }
}
function tick() {
  paint(ratioNow());
  raf = requestAnimationFrame(tick);
}
function ratioFromEvent(e: PointerEvent) {
  const el = trackEl.value;
  if (!el) return 0;
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0) return 0;
  return Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
}
function onPointerDown(e: PointerEvent) {
  trackEl.value?.setPointerCapture(e.pointerId);
  seeking = true;
  seekRatio = ratioFromEvent(e);
  paint(seekRatio);
}
function onPointerMove(e: PointerEvent) {
  if (!seeking) return;
  seekRatio = ratioFromEvent(e);
  paint(seekRatio);
}
function onPointerUp(e: PointerEvent) {
  if (!seeking) return;
  seekRatio = ratioFromEvent(e);
  seek(seekRatio * (player.duration || 0));
  seeking = false;
  paint(ratioNow());
}

/* ---- swipe down / Escape to exit ---- */
let touchY0 = 0;
let touchX0 = 0;
let swiping = false;
const dragY = ref(0);

function onTouchStart(e: TouchEvent) {
  if (e.touches.length !== 1) return;
  const target = e.target as HTMLElement | null;
  // 歌词区纵向滚动时不抢手势
  if (target && listEl.value?.contains(target)) {
    if (listEl.value.scrollTop > 2) return;
  }
  touchY0 = e.touches[0].clientY;
  touchX0 = e.touches[0].clientX;
  swiping = true;
  dragY.value = 0;
}
function onTouchMove(e: TouchEvent) {
  if (!swiping) return;
  const dy = e.touches[0].clientY - touchY0;
  const dx = Math.abs(e.touches[0].clientX - touchX0);
  if (dy > 8 && dy > dx) {
    dragY.value = Math.min(160, dy);
    if (e.cancelable) e.preventDefault();
  } else if (dy < -4 || dx > dy) {
    swiping = false;
    dragY.value = 0;
  }
}
function onTouchEnd() {
  if (!swiping) return;
  if (dragY.value > 90) closeNowPlaying();
  swiping = false;
  dragY.value = 0;
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') closeNowPlaying();
}

onMounted(() => {
  raf = requestAnimationFrame(tick);
  window.addEventListener('keydown', onKey);
});
onUnmounted(() => {
  cancelAnimationFrame(raf);
  window.removeEventListener('keydown', onKey);
});
</script>

<template>
  <div
    ref="rootEl"
    class="np"
    :style="dragY ? { transform: `translateY(${dragY}px)`, opacity: String(1 - dragY / 220) } : undefined"
    @touchstart.passive="onTouchStart"
    @touchmove="onTouchMove"
    @touchend="onTouchEnd"
    @touchcancel="onTouchEnd"
  >
    <div
      v-if="t?.cover"
      class="np-bg"
      :style="{ backgroundImage: `url(${coverSrc(t.cover, '500y500')})` }"
      aria-hidden="true"
    ></div>
    <div class="top">
      <button class="back" type="button" aria-label="返回" title="返回" @click="closeNowPlaying">
        <Icon name="chevron-left" :size="18" />
      </button>
      <div class="grab" aria-hidden="true"></div>
      <div class="top-spacer"></div>
    </div>

    <div class="stage">
      <div class="art">
        <img v-if="t?.cover" :src="coverSrc(t.cover, '500y500')" class="cover" />
        <div v-else class="cover ph"><Icon name="music" :size="56" /></div>
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
            <Icon :name="isLiked(t?.ncmId) ? 'heart-solid' : 'heart'" :size="20" />
          </button>
        </div>
      </div>

      <div class="lyrics" ref="listEl">
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

    <div class="dock">
      <div class="progress">
        <span ref="timeCurEl" class="time">0:00</span>
        <div
          ref="trackEl"
          class="seek"
          role="slider"
          tabindex="0"
          @pointerdown.prevent="onPointerDown"
          @pointermove="onPointerMove"
          @pointerup="onPointerUp"
          @pointercancel="onPointerUp"
        >
          <div class="seek-rail"></div>
          <div ref="fillEl" class="seek-fill"></div>
          <div ref="thumbEl" class="seek-thumb"></div>
        </div>
        <span ref="timeDurEl" class="time">0:00</span>
      </div>
      <div class="ctrls">
        <button class="opt" :class="{ on: player.shuffle }" title="随机" @click="toggleShuffle">
          <Icon name="shuffle" :size="18" />
        </button>
        <button title="上一首" @click="prev"><Icon name="previous" :size="22" /></button>
        <button class="play-toggle" :title="player.playing ? '暂停' : '播放'" @click="togglePlay">
          <Icon
            :name="player.playing ? 'pause' : 'play'"
            :size="26"
            :class="{ 'play-icon': !player.playing }"
          />
        </button>
        <button title="下一首" @click="next()"><Icon name="next" :size="22" /></button>
        <button
          class="opt"
          :class="{ on: player.repeat !== 'off' }"
          :title="repeatLabel"
          @click="cycleRepeat"
        >
          <Icon :name="player.repeat === 'one' ? 'repeat-1' : 'repeat'" :size="18" />
        </button>
      </div>
      <p v-if="player.error" class="err">{{ player.error }}</p>
    </div>
  </div>
</template>

<style scoped>
.np {
  position: fixed;
  isolation: isolate;
  inset: 0;
  z-index: 80;
  background: var(--bg-window);
  display: flex;
  flex-direction: column;
  padding: calc(20px + env(safe-area-inset-top)) 36px calc(24px + env(safe-area-inset-bottom));
  overflow: hidden;
  box-sizing: border-box;
  transition: transform var(--dur-fast) var(--ease-out), opacity var(--dur-fast) var(--ease-out);
  will-change: transform, opacity;
}
.top {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  margin-bottom: 20px;
  flex-shrink: 0;
  position: relative;
  z-index: 2;
}
/* Back: bare icon like play/pause — no gray square/pill fill */
.back {
  justify-self: start;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  min-width: 36px;
  min-height: 36px;
  padding: 0;
  gap: 0;
  background: transparent;
  border: 0;
  color: var(--text-secondary);
  cursor: pointer;
  border-radius: var(--btn-radius);
  font-weight: 550;
  line-height: 0;
  transition: background var(--dur-fast) var(--ease-swift), color var(--dur-fast) var(--ease-swift),
    transform var(--dur-fast) var(--ease-swift);
}
.back:hover {
  color: var(--text);
  background: var(--fill-secondary);
}
.back:active {
  transform: scale(0.97);
}
.grab {
  width: 36px;
  height: 5px;
  border-radius: var(--radius-pill);
  background: var(--border-strong);
  opacity: 0.55;
}
.top-spacer {
  justify-self: end;
}
.stage {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(280px, 40%) 1fr;
  gap: 40px;
  align-items: stretch;
  position: relative;
  z-index: 1;
}
.art {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 20px;
  min-height: 0;
  padding-top: 4px;
}
.cover {
  width: min(100%, 400px);
  max-height: min(400px, 50vh);
  aspect-ratio: 1;
  border-radius: 14px;
  object-fit: cover;
  box-shadow: var(--shadow-lg);
  border: 0.5px solid rgba(0, 0, 0, 0.06);
  animation: sm-scale-in var(--dur-slow) var(--ease-enter);
}
.ph {
  display: grid;
  place-items: center;
  background: var(--fill-secondary);
  color: var(--muted);
}
.meta {
  text-align: center;
  max-width: 400px;
  width: 100%;
}
.title {
  font-family: var(--font-display);
  font-size: 26px;
  font-weight: 500;
  letter-spacing: -0.02em;
  line-height: 1.25;
}
.sub {
  color: var(--text-secondary);
  margin-top: 8px;
  font-size: 15px;
  opacity: 0.85;
}
.like {
  margin-top: 16px;
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  padding: 10px;
  border-radius: var(--btn-radius);
  transition: color var(--dur-fast) var(--ease-swift), transform var(--dur-fast) var(--ease-swift), background var(--dur-fast) var(--ease-swift);
}
.like:hover { background: var(--fill-secondary); color: var(--text); }
.like:active {
  transform: scale(0.9);
}
.like.on {
  color: var(--accent);
}
.lyrics {
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 28% 20px;
  mask-image: linear-gradient(transparent, #000 14%, #000 86%, transparent);
  -webkit-mask-image: linear-gradient(transparent, #000 14%, #000 86%, transparent);
}
.line {
  padding: 14px 12px;
  font-size: 18px;
  color: var(--muted);
  cursor: pointer;
  border-radius: var(--radius-md);
  letter-spacing: -0.01em;
  transition: color var(--dur) var(--ease-swift), transform var(--dur) var(--ease-swift),
    background var(--dur-fast) var(--ease-swift), font-size var(--dur) var(--ease-swift), font-weight var(--dur) var(--ease-swift);
}
.line.on {
  color: var(--text);
  font-weight: 700;
  font-size: 26px;
  letter-spacing: -0.025em;
}
.line:hover {
  background: var(--fill-secondary);
}
.trans {
  font-size: 0.82em;
  opacity: 0.7;
  margin-top: 4px;
  font-weight: 500;
}
.muted {
  color: var(--muted);
  text-align: center;
}
.np-bg {
  position: absolute;
  inset: -40px;
  background-size: cover;
  background-position: center;
  filter: blur(48px) saturate(1.15);
  opacity: 0.28;
  transform: scale(1.08);
  pointer-events: none;
  z-index: 0;
}
[data-theme='light'] .np-bg { opacity: 0.18; }
.np > .top,
.np > .stage,
.np > .dock {
  position: relative;
  z-index: 1;
}
.dock {
  /* Blend into cover/lyrics page — no floating card chrome */
  margin-top: 20px;
  padding: 12px 8px 4px;
  max-width: 520px;
  width: 100%;
  margin-left: auto;
  margin-right: auto;
  background: transparent;
  border: 0;
  border-radius: 0;
  box-shadow: none;
  -webkit-backdrop-filter: none;
  backdrop-filter: none;
  position: relative;
  z-index: 1;
}
.progress {
  display: flex;
  gap: 12px;
  align-items: center;
  font-size: 12px;
  color: var(--muted);
}
.time {
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'tnum' 1;
  min-width: 36px;
  text-align: center;
  opacity: 0.7;
}
.seek {
  position: relative;
  flex: 1;
  height: 22px;
  cursor: pointer;
  touch-action: none;
}
.seek-rail,
.seek-fill {
  position: absolute;
  left: 0;
  right: 0;
  top: 50%;
  height: 4px;
  margin-top: -2px;
  border-radius: 2px;
  pointer-events: none;
}
.seek-rail {
  background: var(--border-strong);
  opacity: 0.5;
}
.seek-fill {
  right: auto;
  width: 100%;
  background: var(--accent);
  transform: scaleX(0);
  transform-origin: left center;
}
.seek-thumb {
  position: absolute;
  top: 50%;
  width: 14px;
  height: 14px;
  margin: -7px 0 0 -7px;
  border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 0 2px var(--surface-1);
  left: 0%;
  pointer-events: none;
}
.ctrls {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 20px;
  margin-top: 12px;
}
.ctrls button {
  background: none;
  border: 0;
  color: inherit;
  cursor: pointer;
  opacity: 0.78;
  padding: 10px;
  border-radius: var(--btn-radius);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 0;
  min-width: 44px;
  min-height: 44px;
  transition: background var(--dur-fast) var(--ease-swift), opacity var(--dur-fast) var(--ease-swift),
    transform var(--dur-fast) var(--ease-swift), color var(--dur-fast) var(--ease-swift);
}
.ctrls button:hover {
  opacity: 1;
  background: var(--fill-secondary);
}
.ctrls button:active {
  transform: scale(0.9);
}
.ctrls button:active :deep(.icon) {
  animation: icon-pop var(--dur-fast) var(--ease-swift);
}
@keyframes icon-pop {
  0% { transform: scale(1); }
  45% { transform: scale(0.82); }
  100% { transform: scale(1); }
}
.ctrls button.on {
  opacity: 1;
  color: var(--accent);
}
/* Play/pause: same chrome as prev/next — no filled disc/circle behind the icon */
.ctrls .play-toggle {
  opacity: 1;
  color: var(--text);
  background: transparent;
}
.ctrls .play-toggle:hover {
  background: var(--fill-secondary);
  opacity: 1;
}
.ctrls .play-toggle .play-icon {
  transform: translateX(var(--play-optical-x));
}
.err {
  color: #c62f2f;
  font-size: 12px;
  text-align: center;
  margin-top: 6px;
}

@media (max-width: 900px) {
  .stage {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto minmax(0, 1fr);
    gap: 16px;
  }
  .art {
    flex-direction: row;
    align-items: center;
    gap: 16px;
    justify-content: flex-start;
    padding-top: 0;
  }
  .cover {
    width: 120px;
    height: 120px;
    max-height: 120px;
    flex-shrink: 0;
    border-radius: 12px;
  }
  .meta {
    text-align: left;
    min-width: 0;
  }
  .title {
    font-size: 20px;
  }
  .grab {
    display: block;
  }
}

@media (max-width: 700px) {
  .np {
    padding: calc(10px + env(safe-area-inset-top)) 18px calc(14px + env(safe-area-inset-bottom));
  }
  .back {
    width: 40px;
    height: 40px;
    min-width: 40px;
    min-height: 40px;
  }
  .top {
    margin-bottom: 16px;
  }
  .cover {
    width: 96px;
    height: 96px;
    max-height: 96px;
  }
  .ctrls {
    gap: 24px;
  }
  .ctrls .play-toggle {
    min-width: 48px;
    min-height: 48px;
  }
  .seek {
    height: 28px;
  }
}

@media (min-width: 901px) {
  .grab {
    display: none;
  }
}
</style>
