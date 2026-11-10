<script setup lang="ts">
import { dialog } from '../lib/dialog';
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
import {
  isWinNowPlayingChrome,
  setNowPlayingTitlebar,
  winClose,
  winMinimize,
  winToggleMaximize,
} from '../lib/titlebar';
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
const badCover = ref<string | null>(null);
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

function isDocHidden() {
  return typeof document !== 'undefined' && document.hidden;
}

/** 关窗隐藏 / 切到后台时置位；回到前台再对齐当前行。勿把普通聚焦当成手动离词。 */
let lyricsSuspended = false;
let unlistenFocus: (() => void) | null = null;
let blurCheckTimer = 0;
let npMounted = false;
/** 初次对齐完成前用即时跳转，避免重进时从列表顶部 smooth 滚到当前行。 */
let lyricsFollowSettled = false;
let alignToken = 0;
let alignTimer = 0;
let lyricsRo: ResizeObserver | null = null;
let lastLyricsHeight = 0;

function clearAlignTimer() {
  if (alignTimer) {
    window.clearTimeout(alignTimer);
    alignTimer = 0;
  }
}

function activeLineEl(root: HTMLElement | null, i: number) {
  if (!root || i < 0) return null;
  return root.querySelector(`[data-i="${i}"]`) as HTMLElement | null;
}

function frame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

async function scrollToActive(behavior: ScrollBehavior = 'smooth'): Promise<boolean> {
  if (isDocHidden()) return false;
  await nextTick();
  const root = listEl.value;
  const i = activeIdx.value;
  const el = activeLineEl(root, i);
  if (!root || !el) return false;
  // 入场动画 / 歌词刚插入时，列表或行可能尚未完成布局
  if (root.clientHeight <= 0 || el.offsetHeight <= 0) return false;
  const top = el.offsetTop - root.clientHeight / 2 + el.offsetHeight / 2;
  root.scrollTo({ top: Math.max(0, top), behavior });
  return true;
}

/**
 * 挂载或歌词就绪后对齐当前行，并在布局 / 入场动画完成后再跳一次。
 * 播放页是 v-if 重建，列表从 scrollTop 0 开始；若只在 nextTick 滚一次，
 * 行还没量到尺寸或 sm-slide-up 结束重置滚动时就会停在开头，
 * 且 activeIdx 不变就不会再触发跟随。
 */
function alignToActive(behavior: ScrollBehavior = 'auto') {
  const token = ++alignToken;
  clearAlignTimer();

  const attempt = () => {
    if (token !== alignToken || !npMounted || isDocHidden()) return Promise.resolve(false);
    return scrollToActive(behavior);
  };

  void (async () => {
    const deadline = performance.now() + 2000;
    let ok = await attempt();
    while (!ok && token === alignToken && npMounted && !isDocHidden() && performance.now() < deadline) {
      await frame();
      ok = await attempt();
    }
    if (token !== alignToken) return;
    // 布局可能在随后几帧才稳定；入场动画结束时 transform 撤销也可能把 scrollTop 打回 0
    await frame();
    if (token !== alignToken) return;
    ok = (await attempt()) || ok;
    await frame();
    if (token !== alignToken) return;
    ok = (await attempt()) || ok;
    alignTimer = window.setTimeout(() => {
      alignTimer = 0;
      if (token !== alignToken || !npMounted || isDocHidden()) return;
      void scrollToActive(behavior).then((done) => {
        if (done && token === alignToken) lyricsFollowSettled = true;
      });
    }, 360);
    if (ok) lyricsFollowSettled = true;
  })();
}

function bindLyricsObserver() {
  unbindLyricsObserver();
  const root = listEl.value;
  if (!root || typeof ResizeObserver === 'undefined') return;
  lastLyricsHeight = root.clientHeight;
  lyricsRo = new ResizeObserver(() => {
    const el = listEl.value;
    if (!el) return;
    const h = el.clientHeight;
    if (h <= 0 || h === lastLyricsHeight) return;
    lastLyricsHeight = h;
    // 列表从 0 高变为可见，或入场/窗口变化改了高度：立刻对齐，勿等下一句
    alignToActive('auto');
  });
  lyricsRo.observe(root);
}

function unbindLyricsObserver() {
  lyricsRo?.disconnect();
  lyricsRo = null;
}

function suspendLyricsFollow() {
  lyricsSuspended = true;
  stopAnim();
}

function resumeLyricsFollow(force = false) {
  if (isDocHidden()) return;
  startAnim();
  if (!force && !lyricsSuspended) return;
  lyricsSuspended = false;
  // 隐藏期间 activeIdx 可能已走过很多行，立刻跳到当前行，而不是等下一句
  alignToActive('auto');
}

watch(activeIdx, () => {
  // 后台 / 隐藏时不滚歌词，避免无意义 smooth scroll + 合成
  if (isDocHidden() || lyricsSuspended) return;
  // 重进尚未对齐时不要从顶部慢慢滚，否则会闪回歌曲开头
  void scrollToActive(lyricsFollowSettled ? 'smooth' : 'auto');
});

watch(
  () => [loading.value, lines.value.length] as const,
  () => {
    if (!npMounted || loading.value || !lines.value.length) return;
    alignToActive('auto');
  },
);

async function onLike() {
  const id = t.value?.ncmId;
  if (id == null || !session.profile) return;
  try {
    await toggleLike(id);
  } catch (e) {
    void dialog.alert(String(e));
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
let animRunning = false;
function stopAnim() {
  animRunning = false;
  if (raf) {
    cancelAnimationFrame(raf);
    raf = 0;
  }
}
function startAnim() {
  if (animRunning) return;
  if (typeof document !== 'undefined' && document.hidden) return;
  animRunning = true;
  raf = requestAnimationFrame(tick);
}
function tick() {
  if (!animRunning) return;
  paint(ratioNow());
  raf = requestAnimationFrame(tick);
}
function onVisibility() {
  if (isDocHidden()) suspendLyricsFollow();
  else resumeLyricsFollow(true);
}

function onWindowFocus() {
  resumeLyricsFollow();
}

function onWindowBlur() {
  if (blurCheckTimer) window.clearTimeout(blurCheckTimer);
  // hide() 可能稍晚于 blur；短延迟后再查窗口是否真的不可见
  blurCheckTimer = window.setTimeout(() => {
    blurCheckTimer = 0;
    void (async () => {
      if (isDocHidden()) {
        suspendLyricsFollow();
        return;
      }
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        if (!(await getCurrentWindow().isVisible())) suspendLyricsFollow();
      } catch {
        /* 非 Tauri / 查询失败：仅依赖 visibilitychange */
      }
    })();
  }, 80);
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

const showWinChrome = isWinNowPlayingChrome();

function onTitleDblClick() {
  if (!showWinChrome) return;
  void winToggleMaximize();
}

function onNpAnimationEnd(e: AnimationEvent) {
  if (e.target !== rootEl.value) return;
  if (!npMounted || !session.nowPlaying) return;
  // sm-slide-up 结束、transform 撤销后再跳一次，避免滚动被重置到顶部
  alignToActive('auto');
}

onMounted(() => {
  // 打开时已在 openNowPlaying 切 overlay；这里再确保一次（含直接挂载路径）
  npMounted = true;
  lyricsFollowSettled = false;
  void setNowPlayingTitlebar(true);
  startAnim();
  document.addEventListener('visibilitychange', onVisibility);
  window.addEventListener('focus', onWindowFocus);
  window.addEventListener('blur', onWindowBlur);
  window.addEventListener('keydown', onKey);
  rootEl.value?.addEventListener('animationend', onNpAnimationEnd);
  bindLyricsObserver();
  // 中途打开播放页（含关窗后再开 / 离开再进入）时对齐当前行，并等到布局就绪
  alignToActive('auto');
  void (async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const unlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        if (focused) onWindowFocus();
        else onWindowBlur();
      });
      if (!npMounted) {
        unlisten();
        return;
      }
      unlistenFocus = unlisten;
    } catch {
      /* 浏览器预览无窗口 API */
    }
  })();
});
onUnmounted(() => {
  npMounted = false;
  alignToken += 1;
  lyricsFollowSettled = false;
  clearAlignTimer();
  unbindLyricsObserver();
  stopAnim();
  if (blurCheckTimer) {
    window.clearTimeout(blurCheckTimer);
    blurCheckTimer = 0;
  }
  document.removeEventListener('visibilitychange', onVisibility);
  window.removeEventListener('focus', onWindowFocus);
  window.removeEventListener('blur', onWindowBlur);
  window.removeEventListener('keydown', onKey);
  rootEl.value?.removeEventListener('animationend', onNpAnimationEnd);
  unlistenFocus?.();
  unlistenFocus = null;
  // Transition leave 结束后再恢复系统标题栏
  void setNowPlayingTitlebar(false);
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
      v-if="t?.cover && badCover !== t.cover"
      class="np-bg"
      :style="{ backgroundImage: `url(${coverSrc(t.cover, '500y500')})` }"
      aria-hidden="true"
    ></div>
    <!-- 桌面沉浸：顶栏全宽拖拽条（返回键在其下方，保持可点） -->
    <div
      class="np-title-drag"
      data-tauri-drag-region
      aria-hidden="true"
      @dblclick="onTitleDblClick"
    ></div>
    <div v-if="showWinChrome" class="np-win-controls">
      <button type="button" class="np-win-btn" title="最小化" aria-label="最小化" @click="winMinimize">
        <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
          <rect x="1" y="5.5" width="10" height="1" fill="currentColor" />
        </svg>
      </button>
      <button type="button" class="np-win-btn" title="最大化" aria-label="最大化" @click="winToggleMaximize">
        <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
          <rect x="1.5" y="1.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>
      <button type="button" class="np-win-btn close" title="关闭" aria-label="关闭" @click="winClose">
        <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
          <path d="M2 2l8 8M10 2L2 10" stroke="currentColor" stroke-width="1.2" fill="none" />
        </svg>
      </button>
    </div>
    <div class="top">
      <button class="back" type="button" aria-label="返回" title="返回" @click="closeNowPlaying">
        <Icon name="chevron-left" :size="18" />
      </button>
      <div class="grab" data-tauri-drag-region aria-hidden="true"></div>
      <div class="top-spacer" data-tauri-drag-region></div>
    </div>

    <div class="stage">
      <div class="art">
        <img
          v-if="t?.cover && badCover !== t.cover"
          :src="coverSrc(t.cover, '500y500')"
          class="cover"
          @error="badCover = t.cover"
        />
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
  margin-bottom: var(--space-5);
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
  gap: var(--space-6);
  align-items: stretch;
  position: relative;
  z-index: 1;
}
.art {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-5);
  min-height: 0;
  padding-top: var(--space-1);
}
.cover {
  width: min(100%, 400px);
  max-height: min(400px, 50vh);
  aspect-ratio: 1;
  border-radius: var(--radius-xl);
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
  margin-top: var(--space-2);
  font-size: 15px;
  opacity: 0.85;
}
.like {
  margin-top: var(--space-4);
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
  padding: var(--space-3);
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
  margin-top: var(--space-5);
  padding: var(--space-3) var(--space-2) var(--space-1);
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
  gap: var(--space-3);
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
  gap: var(--space-5);
  margin-top: var(--space-3);
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
    border-radius: var(--radius-lg);
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

<style>
/*
 * Desktop Now Playing immersive chrome — MUST be unscoped.
 * Vue scoped `:global(html.sm-np-*) .child` was compiled away before (0.11.5).
 */
.np-title-drag {
  display: none;
  pointer-events: none;
}
.np-win-controls {
  display: none;
}
html.sm-np-overlay {
  --np-mac-titlebar: 56px;
}
html.sm-np-overlay .np-title-drag {
  display: block;
  pointer-events: auto;
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: var(--np-mac-titlebar);
  z-index: 5;
}
html.sm-np-overlay .np {
  padding-top: calc(var(--np-mac-titlebar) + env(safe-area-inset-top, 0px));
}
html.sm-np-overlay .np > .top {
  position: relative;
  z-index: 6;
}

/* Windows：去系统标题栏后自绘拖拽条 + 最小/最大/关闭 */
html.sm-np-win {
  --np-win-titlebar: 36px;
}
html.sm-np-win .np-title-drag {
  display: block;
  pointer-events: auto;
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: var(--np-win-titlebar);
  z-index: 5;
}
html.sm-np-win .np-win-controls {
  display: flex;
  position: absolute;
  top: 0;
  right: 0;
  height: var(--np-win-titlebar);
  z-index: 7;
  align-items: stretch;
}
html.sm-np-win .np-win-btn {
  width: 46px;
  height: 100%;
  border: 0;
  margin: 0;
  padding: 0;
  border-radius: 0;
  background: transparent;
  color: var(--text, #e8e8e8);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: default;
  opacity: 0.85;
}
html.sm-np-win .np-win-btn:hover {
  background: color-mix(in srgb, #fff 14%, transparent);
  opacity: 1;
}
html.sm-np-win .np-win-btn.close:hover {
  background: #c42b1c;
  color: #fff;
}
html.sm-np-win .np {
  padding-top: calc(var(--np-win-titlebar) + env(safe-area-inset-top, 0px));
}
html.sm-np-win .np > .top {
  position: relative;
  z-index: 6;
}
</style>
