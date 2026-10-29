<script setup lang="ts">
import { computed } from 'vue';
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
import SourceBadge from './SourceBadge.vue';
import Icon from './Icon.vue';

const t = computed(() => current());
const repeatLabel = computed(
  () => ({ off: '不循环', all: '列表循环', one: '单曲循环' })[player.repeat],
);
const volumePct = computed(() => Math.round(player.volume * 100));
const volumeIcon = computed(() => (player.volume <= 0.001 ? 'volume-mute' : 'volume'));
</script>

<template>
  <footer class="bar">
    <div class="now">
      <img v-if="t?.cover" :src="t.cover" class="cover" />
      <div v-else class="cover ph"><Icon name="music" :size="18" /></div>
      <div v-if="t" class="info">
        <div class="title">{{ t.title }} <SourceBadge :source="t.source" /></div>
        <div class="sub">{{ t.artists.join(' / ') }}</div>
      </div>
      <div v-else class="sub">未在播放</div>
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
        <span>{{ fmtTime(player.currentTime) }}</span>
        <input
          type="range"
          min="0"
          :max="player.duration || 0"
          step="0.5"
          :value="player.currentTime"
          @input="seek(+($event.target as HTMLInputElement).value)"
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
.progress input {
  flex: 1;
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
