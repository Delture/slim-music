<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import {
  current,
  cycleRepeat,
  next,
  player,
  prev,
  seek,
  setVolume,
  togglePlay,
  toggleShuffle,
} from '../lib/player';
import { coverSrc, fmtTime } from '../lib/format';
import { isLiked, openNowPlaying, session, toggleLike } from '../lib/session';
import SourceBadge from './SourceBadge.vue';
import Icon from './Icon.vue';

const t = computed(() => current());
const repeatLabel = computed(
  () => ({ off: '不循环', all: '列表循环', one: '单曲循环' })[player.repeat],
);
const volumePct = computed(() => Math.round(player.volume * 100));
const volumeIcon = computed(() => (player.volume <= 0.001 ? 'volume-mute' : 'volume'));

/**
 * 自定义进度条：不用 <input type=range>（WebKit 填充轨会每帧重算导致整条左右抖）。
 * 只通过 rAF 写 transform / textContent，拖动时 gated，无 CSS transition。
 */
const trackEl = ref<HTMLElement | null>(null);
const fillEl = ref<HTMLElement | null>(null);
const thumbEl = ref<HTMLElement | null>(null);
const timeCurEl = ref<HTMLElement | null>(null);
const timeDurEl = ref<HTMLElement | null>(null);

let seeking = false;
let seekRatio = 0;
let raf = 0;
let lastCurText = '';
let lastDurText = '';

function ratioNow(): number {
  if (seeking) return seekRatio;
  const d = player.duration;
  if (!d || !isFinite(d) || d <= 0) return 0;
  return Math.min(1, Math.max(0, player.currentTime / d));
}

function paint(r: number) {
  // scaleX 不触发 layout；transform-origin:left
  if (fillEl.value) fillEl.value.style.transform = `scaleX(${r})`;
  if (thumbEl.value) thumbEl.value.style.left = `${r * 100}%`;
  const cur = seeking ? r * (player.duration || 0) : player.currentTime;
  const curT = fmtTime(cur);
  const durT = fmtTime(player.duration);
  if (timeCurEl.value && curT !== lastCurText) {
    timeCurEl.value.textContent = curT;
    lastCurText = curT;
  }
  if (timeDurEl.value && durT !== lastDurText) {
    timeDurEl.value.textContent = durT;
    lastDurText = durT;
  }
}

function tick() {
  paint(ratioNow());
  if (!volDragging) paintVolume(player.volume);
  raf = requestAnimationFrame(tick);
}

function ratioFromEvent(e: PointerEvent): number {
  const el = trackEl.value;
  if (!el) return 0;
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0) return 0;
  return Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
}

function onPointerDown(e: PointerEvent) {
  const el = trackEl.value;
  if (!el) return;
  el.setPointerCapture(e.pointerId);
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
  const dur = player.duration || 0;
  seek(seekRatio * dur);
  seeking = false;
  paint(ratioNow());
}

/** 音量条：同进度条 scaleX + thumb，不走 native range */
const volTrackEl = ref<HTMLElement | null>(null);
const volFillEl = ref<HTMLElement | null>(null);
const volThumbEl = ref<HTMLElement | null>(null);
let volDragging = false;

function paintVolume(r: number) {
  const v = Math.min(1, Math.max(0, r));
  if (volFillEl.value) volFillEl.value.style.transform = `scaleX(${v})`;
  if (volThumbEl.value) volThumbEl.value.style.left = `${v * 100}%`;
}

function volRatioFromEvent(e: PointerEvent): number {
  const el = volTrackEl.value;
  if (!el) return player.volume;
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0) return player.volume;
  return Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
}

function onVolDown(e: PointerEvent) {
  const el = volTrackEl.value;
  if (!el) return;
  el.setPointerCapture(e.pointerId);
  volDragging = true;
  const r = volRatioFromEvent(e);
  paintVolume(r);
  setVolume(r);
}

function onVolMove(e: PointerEvent) {
  if (!volDragging) return;
  const r = volRatioFromEvent(e);
  paintVolume(r);
  setVolume(r);
}

function onVolUp(e: PointerEvent) {
  if (!volDragging) return;
  const r = volRatioFromEvent(e);
  paintVolume(r);
  setVolume(r);
  volDragging = false;
}

onMounted(() => {
  paintVolume(player.volume);
  raf = requestAnimationFrame(tick);
});
onUnmounted(() => cancelAnimationFrame(raf));

function openNp() {
  if (t.value) openNowPlaying();
}

async function onLike() {
  const id = t.value?.ncmId;
  if (id == null || !session.profile) return;
  try {
    await toggleLike(id);
  } catch (err) {
    alert(String(err));
  }
}
</script>

<template>
  <footer class="bar">
    <div class="now" title="打开播放页" @click="openNp">
      <img v-if="t?.cover" :src="coverSrc(t.cover)" class="cover" />
      <div v-else class="cover ph"><Icon name="music" :size="18" /></div>
      <div v-if="t" class="info">
        <div class="title">{{ t.title }} <SourceBadge :source="t.source" /></div>
        <div class="sub">{{ t.artists.join(' / ') }}</div>
      </div>
      <div v-else class="sub">未在播放</div>
      <button
        v-if="t?.source === 'netease' && session.profile"
        class="heart"
        :class="{ on: isLiked(t.ncmId) }"
        title="喜欢"
        @click.stop="onLike"
      >
        <Icon :name="isLiked(t.ncmId) ? 'heart-solid' : 'heart'" :size="15" />
      </button>
    </div>

    <div class="center">
      <div class="ctrls">
        <button class="opt" :class="{ on: player.shuffle }" title="随机" @click="toggleShuffle">
          <Icon name="shuffle" :size="15" />
        </button>
        <button class="opt" title="上一首" @click="prev"><Icon name="previous" :size="16" /></button>
        <button class="big" :title="player.playing ? '暂停' : '播放'" @click="togglePlay">
          <Icon :name="player.playing ? 'pause' : 'play'" :size="22" :class="{ 'play-icon': !player.playing }" />
        </button>
        <button title="下一首" @click="next()"><Icon name="next" :size="16" /></button>
        <button
          class="opt"
          :class="{ on: player.repeat !== 'off' }"
          :title="repeatLabel"
          @click="cycleRepeat"
        >
          <Icon :name="player.repeat === 'one' ? 'repeat-1' : 'repeat'" :size="15" />
        </button>
      </div>
      <div class="progress">
        <span ref="timeCurEl" class="time">0:00</span>
        <div
          ref="trackEl"
          class="seek"
          role="slider"
          :aria-valuenow="player.currentTime"
          :aria-valuemax="player.duration || 0"
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
      <div v-if="player.error" class="err">{{ player.error }}</div>
    </div>

    <div class="vol">
      <Icon :name="volumeIcon" :size="15" />
      <div
        ref="volTrackEl"
        class="vol-seek"
        role="slider"
        :aria-valuenow="volumePct"
        aria-valuemin="0"
        aria-valuemax="100"
        :title="`音量 ${volumePct}%`"
        tabindex="0"
        @pointerdown.prevent="onVolDown"
        @pointermove="onVolMove"
        @pointerup="onVolUp"
        @pointercancel="onVolUp"
      >
        <div class="seek-rail"></div>
        <div ref="volFillEl" class="seek-fill"></div>
        <div ref="volThumbEl" class="seek-thumb"></div>
      </div>
      <span class="pct">{{ volumePct }}%</span>
      <span class="sub queue">{{
        player.queue.length ? `${player.pos + 1}/${player.queue.length}` : ''
      }}</span>
    </div>
  </footer>
</template>

<style scoped>
.bar {
  display: grid;
  grid-template-columns: 1fr 1.4fr 1fr;
  align-items: center;
  gap: 16px;
  padding: 10px 20px;
  min-height: 72px;
  background: var(--surface-player);
  -webkit-backdrop-filter: var(--vibrancy-strong);
  backdrop-filter: var(--vibrancy-strong);
  box-shadow: none;
  border: 0;
  z-index: 20;
}
.now {
  display: flex;
  gap: 12px;
  align-items: center;
  min-width: 0;
  cursor: pointer;
  border-radius: var(--radius-md);
  padding: 4px 6px;
  transition: background var(--dur-fast) var(--ease-swift);
}
.now:hover {
  background: var(--fill-secondary);
}
.cover {
  width: 52px;
  height: 52px;
  border-radius: 8px;
  object-fit: cover;
  transition: transform var(--dur) var(--ease-swift);
  box-shadow: var(--shadow-cover);
  border: 0.5px solid rgba(0, 0, 0, 0.06);
}
.now:hover .cover {
  transform: scale(1.03);
}
.ph {
  display: grid;
  place-items: center;
  background: var(--fill-secondary);
  color: var(--muted);
  box-shadow: none;
}
.info {
  min-width: 0;
  flex: 1;
}
.title,
.sub {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.title {
  font-size: 15px;
  font-weight: 600;
  letter-spacing: -0.015em;
  opacity: 0.92;
}
.sub {
  color: var(--text-secondary);
  font-size: 12px;
  margin-top: 2px;
  opacity: 0.85;
}
.heart {
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  padding: 8px;
  line-height: 0;
  border-radius: 50%;
  min-width: 36px;
  min-height: 36px;
  transition: color var(--dur-fast) var(--ease-swift), background var(--dur-fast) var(--ease-swift);
}
.heart:hover { background: var(--hover); color: var(--text); }
.heart.on {
  color: var(--accent);
}
.center {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
}
.ctrls {
  display: flex;
  align-items: center;
  gap: 6px;
}
.ctrls button {
  background: none;
  border: 0;
  color: inherit;
  cursor: pointer;
  opacity: 0.72;
  padding: 8px;
  border-radius: 50%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 0;
  min-width: 36px;
  min-height: 36px;
  transition: background var(--dur-fast) var(--ease-swift), opacity var(--dur-fast) var(--ease-swift), transform var(--dur-fast) var(--ease-swift), color var(--dur-fast) var(--ease-swift);
}
.ctrls button:hover {
  opacity: 1;
  background: var(--fill-secondary);
}
.ctrls button:active {
  transform: scale(0.92);
}
.ctrls button.on {
  opacity: 1;
  color: var(--accent);
}
.ctrls .big {
  opacity: 1;
  width: 42px;
  height: 42px;
  min-width: 42px;
  min-height: 42px;
  background: var(--text);
  color: var(--bg);
}
.ctrls .big :deep(svg),
.ctrls .big > * {
  display: block;
  color: inherit;
}
/* 三角形视觉重心偏左，右移/上移做光学居中 */
.ctrls .big .play-icon {
  transform: translate(2px, -1px);
}
.ctrls .big:hover {
  background: var(--text);
  opacity: 0.88;
}
.progress {
  display: flex;
  gap: 10px;
  align-items: center;
  width: 100%;
  font-size: 11px;
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
  height: 18px;
  cursor: pointer;
  touch-action: none;
  transition: none;
}
.seek-rail,
.seek-fill {
  position: absolute;
  left: 0;
  right: 0;
  top: 50%;
  height: 3px;
  margin-top: -1.5px;
  border-radius: 2px;
  transition: none;
  pointer-events: none;
}
.seek-rail {
  background: var(--border-strong);
  opacity: 0.55;
}
.seek-fill {
  right: auto;
  width: 100%;
  background: var(--accent);
  transform: scaleX(0);
  transform-origin: left center;
  will-change: transform;
  opacity: 1;
}
.seek-thumb {
  position: absolute;
  top: 50%;
  width: 12px;
  height: 12px;
  margin: -6px 0 0 -6px;
  border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 0 2px var(--surface-player-solid);
  left: 0%;
  transition: none;
  pointer-events: none;
  will-change: left;
  opacity: 0;
}
.seek:hover .seek-thumb,
.seek:active .seek-thumb {
  opacity: 1;
}
.err {
  color: #c62f2f;
  font-size: 12px;
}
.vol {
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: flex-end;
  color: var(--muted);
  min-width: 0;
}
.vol-seek {
  position: relative;
  width: 90px;
  height: 16px;
  cursor: pointer;
  touch-action: none;
  transition: none;
  flex-shrink: 0;
}
.vol-seek .seek-rail,
.vol-seek .seek-fill {
  position: absolute;
  left: 0;
  right: 0;
  top: 50%;
  height: 3px;
  margin-top: -1.5px;
  border-radius: 2px;
  transition: none;
  pointer-events: none;
}
.vol-seek .seek-rail {
  background: var(--border-strong);
  opacity: 0.55;
}
.vol-seek .seek-fill {
  right: auto;
  width: 100%;
  background: var(--text);
  opacity: 0.55;
  transform: scaleX(0);
  transform-origin: left center;
  will-change: transform;
}
.vol-seek .seek-thumb {
  position: absolute;
  top: 50%;
  width: 12px;
  height: 12px;
  margin: -6px 0 0 -6px;
  border-radius: 50%;
  background: var(--text);
  box-shadow: 0 0 0 2px var(--surface-player-solid);
  left: 0%;
  transition: none;
  pointer-events: none;
  will-change: left;
  opacity: 0;
}
.vol-seek:hover .seek-thumb,
.vol-seek:active .seek-thumb {
  opacity: 1;
}
.pct {
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  min-width: 36px;
  text-align: right;
  color: var(--muted);
}
.queue {
  min-width: 40px;
  text-align: right;
  font-size: 12px;
}

/* 手机：底部迷你播放条 */
@media (max-width: 700px) {
  .bar {
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 8px;
    padding: 8px 10px;
    min-height: 64px;
  }
  .vol { display: none; }
  .cover { width: 44px; height: 44px; }
  .heart { padding: 10px; min-width: 44px; min-height: 44px; }
  .center .progress, .ctrls .opt { display: none; }
  .ctrls button { min-width: 44px; min-height: 44px; }
  .ctrls .big { width: 44px; height: 44px; min-width: 44px; min-height: 44px; }
  .err { display: none; }
  .seek { height: 28px; }
  .seek-thumb { width: 16px; height: 16px; margin: -8px 0 0 -8px; opacity: 1; }
}
</style>
