<script setup lang="ts">
import { computed } from 'vue';
import { current, cycleRepeat, next, player, prev, seek, setVolume, togglePlay, toggleShuffle } from '../lib/player';
import { fmtTime } from '../lib/format';
import SourceBadge from './SourceBadge.vue';

const t = computed(() => current());
const repeatLabel = computed(() => ({ off: '不循环', all: '列表循环', one: '单曲循环' })[player.repeat]);
</script>

<template>
  <footer class="bar">
    <div class="now">
      <img v-if="t?.cover" :src="t.cover" class="cover" />
      <div v-else class="cover ph">♪</div>
      <div v-if="t" class="info">
        <div class="title">{{ t.title }} <SourceBadge :source="t.source" /></div>
        <div class="sub">{{ t.artists.join(' / ') }}</div>
      </div>
      <div v-else class="sub">未在播放</div>
    </div>

    <div class="center">
      <div class="ctrls">
        <button :class="{ on: player.shuffle }" title="随机" @click="toggleShuffle">🔀</button>
        <button title="上一首" @click="prev">⏮</button>
        <button class="big" @click="togglePlay">{{ player.playing ? '⏸' : '▶' }}</button>
        <button title="下一首" @click="next()">⏭</button>
        <button :class="{ on: player.repeat !== 'off' }" :title="repeatLabel" @click="cycleRepeat">
          {{ player.repeat === 'one' ? '🔂' : '🔁' }}
        </button>
      </div>
      <div class="progress">
        <span>{{ fmtTime(player.currentTime) }}</span>
        <input type="range" min="0" :max="player.duration || 0" step="0.5" :value="player.currentTime"
          @input="seek(+($event.target as HTMLInputElement).value)" />
        <span>{{ fmtTime(player.duration) }}</span>
      </div>
      <div v-if="player.error" class="err">{{ player.error }}</div>
    </div>

    <div class="vol">
      🔊 <input type="range" min="0" max="1" step="0.01" :value="player.volume"
        @input="setVolume(+($event.target as HTMLInputElement).value)" />
      <span class="sub">{{ player.queue.length ? `${player.pos + 1}/${player.queue.length}` : '' }}</span>
    </div>
  </footer>
</template>

<style scoped>
.bar { display: grid; grid-template-columns: 1fr 1.4fr 1fr; align-items: center; gap: 12px; padding: 8px 16px; border-top: 1px solid var(--border); background: var(--bg); }
.now { display: flex; gap: 10px; align-items: center; min-width: 0; }
.cover { width: 48px; height: 48px; border-radius: 6px; object-fit: cover; }
.ph { display: grid; place-items: center; background: var(--hover); }
.info { min-width: 0; }
.title, .sub { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.sub { color: var(--muted); font-size: 12px; }
.center { display: flex; flex-direction: column; align-items: center; gap: 4px; }
.ctrls button { background: none; border: 0; font-size: 18px; cursor: pointer; opacity: .7; padding: 2px 8px; }
.ctrls button.on, .ctrls button:hover { opacity: 1; }
.ctrls .big { font-size: 24px; opacity: 1; }
.progress { display: flex; gap: 8px; align-items: center; width: 100%; font-size: 12px; color: var(--muted); }
.progress input { flex: 1; }
.err { color: #c62f2f; font-size: 12px; }
.vol { display: flex; gap: 6px; align-items: center; justify-content: flex-end; }
</style>
