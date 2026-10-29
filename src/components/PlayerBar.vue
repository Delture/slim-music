<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
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
import { fmtTime } from '../lib/format';
import { isLiked, openNowPlaying, session, toggleLike } from '../lib/session';
import SourceBadge from './SourceBadge.vue';
import Icon from './Icon.vue';

const t = computed(() => current());
const repeatLabel = computed(
  () => ({ off: '不循环', all: '列表循环', one: '单曲循环' })[player.repeat],
);
const volumePct = computed(() => Math.round(player.volume * 100));
const volumeIcon = computed(() => (player.volume <= 0.001 ? 'volume-mute' : 'volume'));

/** 进度条：拖动中不跟 timeupdate 抢值，避免拇指抖动 */
const progressEl = ref<HTMLInputElement | null>(null);
const seeking = ref(false);
const displayTime = ref(0);
let raf = 0;

function syncProgressDom() {
  if (seeking.value) return;
  displayTime.value = player.currentTime;
  const el = progressEl.value;
  if (el) {
    const dur = player.duration || 0;
    if (el.max !== String(dur)) el.max = String(dur);
    // 直接写 DOM，避开 Vue 每帧重绑 :value 导致的拇指回弹
    if (document.activeElement !== el) el.value = String(player.currentTime);
  }
}

function tick() {
  syncProgressDom();
  raf = requestAnimationFrame(tick);
}

onMounted(() => {
  raf = requestAnimationFrame(tick);
});
onUnmounted(() => cancelAnimationFrame(raf));

watch(
  () => player.duration,
  () => syncProgressDom(),
);

function onSeekInput(e: Event) {
  seeking.value = true;
  displayTime.value = +(e.target as HTMLInputElement).value;
}
function onSeekCommit() {
  const el = progressEl.value;
  if (!el) { seeking.value = false; return; }
  const v = +el.value;
  seek(v);
  displayTime.value = v;
  seeking.value = false;
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
    <div class="now" title="打开播放页" @click="t && openNowPlaying()">
      <img v-if="t?.cover" :src="t.cover" class="cover" />
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
        <button :class="{ on: player.shuffle }" title="随机" @click="toggleShuffle">
          <Icon name="shuffle" :size="15" />
        </button>
        <button title="上一首" @click="prev"><Icon name="previous" :size="16" /></button>
        <button class="big" :title="player.playing ? '暂停' : '播放'" @click="togglePlay">
          <Icon :name="player.playing ? 'pause' : 'play'" :size="22" />
        </button>
        <button title="下一首" @click="next()"><Icon name="next" :size="16" /></button>
        <button
          :class="{ on: player.repeat !== 'off' }"
          :title="repeatLabel"
          @click="cycleRepeat"
        >
          <Icon :name="player.repeat === 'one' ? 'repeat-1' : 'repeat'" :size="15" />
        </button>
      </div>
      <div class="progress">
        <span>{{ fmtTime(displayTime) }}</span>
        <input
          ref="progressEl"
          class="seek"
          type="range"
          min="0"
          step="0.05"
          :max="player.duration || 0"
          @input="onSeekInput"
          @change="onSeekCommit"
          @pointerup="onSeekCommit"
        />
        <span>{{ fmtTime(player.duration) }}</span>
      </div>
      <div v-if="player.error" class="err">{{ player.error }}</div>
    </div>

    <div class="vol">
      <Icon :name="volumeIcon" :size="15" />
      <input
        type="range"
        min="0"
        max="1"
        step="0.01"
        :value="player.volume"
        :title="`音量 ${volumePct}%`"
        @input="setVolume(+($event.target as HTMLInputElement).value)"
      />
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
  gap: 12px;
  padding: 10px 16px;
  border-top: 1px solid var(--border);
  background: var(--bg);
}
.now {
  display: flex;
  gap: 10px;
  align-items: center;
  min-width: 0;
  cursor: pointer;
  border-radius: 8px;
  padding: 2px 4px;
}
.now:hover {
  background: var(--hover);
}
.cover {
  width: 48px;
  height: 48px;
  border-radius: 6px;
  object-fit: cover;
}
.ph {
  display: grid;
  place-items: center;
  background: var(--hover);
  color: var(--muted);
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
.sub {
  color: var(--muted);
  font-size: 12px;
}
.heart {
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  padding: 6px;
  line-height: 0;
}
.heart.on {
  color: var(--accent);
}
.center {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}
.ctrls {
  display: flex;
  align-items: center;
  gap: 4px;
}
.ctrls button {
  background: none;
  border: 0;
  color: inherit;
  cursor: pointer;
  opacity: 0.72;
  padding: 6px;
  border-radius: 50%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 0;
}
.ctrls button:hover {
  opacity: 1;
  background: var(--hover);
}
.ctrls button.on {
  opacity: 1;
  color: var(--accent);
}
.ctrls .big {
  opacity: 1;
  width: 40px;
  height: 40px;
  background: var(--hover);
}
.ctrls .big:hover {
  background: var(--border);
}
.progress {
  display: flex;
  gap: 8px;
  align-items: center;
  width: 100%;
  font-size: 12px;
  color: var(--muted);
}
.progress .seek {
  flex: 1;
  /* 播放过程中关掉拇指过渡，避免和 JS 更新打架 */
  transition: none;
}
.progress .seek::-webkit-slider-thumb {
  transition: none;
}
.progress .seek::-moz-range-thumb {
  transition: none;
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
.vol input {
  width: 90px;
}
.pct {
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  min-width: 36px;
  text-align: right;
  color: var(--muted);
}
.queue {
  min-width: 40px;
  text-align: right;
}
</style>
