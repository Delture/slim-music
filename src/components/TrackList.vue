<script setup lang="ts">
// 通用歌曲列表：双击行 / 单击封面或序号 / 右键「从这首开始播放」都会把整张列表设为队列并从该曲开始
import { reactive } from 'vue';
import type { Track } from '../lib/types';
import { current, enqueue, playList } from '../lib/player';
import { fmtTime } from '../lib/format';
import SourceBadge from './SourceBadge.vue';

/** 改成 true 即单击整行就播放 */
const PLAY_ON_SINGLE_CLICK = false;

const props = defineProps<{ tracks: Track[] }>();
const menu = reactive({ show: false, x: 0, y: 0, index: -1 });

function playAt(i: number) {
  playList(props.tracks, i);
}
function openMenu(e: MouseEvent, i: number) {
  Object.assign(menu, { show: true, x: e.clientX, y: e.clientY, index: i });
}
function closeMenu() {
  menu.show = false;
}
function copyName() {
  const t = props.tracks[menu.index];
  if (t) void navigator.clipboard.writeText(`${t.title} - ${t.artists.join('/')}`);
}
</script>

<template>
  <div class="list" @click="closeMenu">
    <div
      v-for="(t, i) in tracks"
      :key="t.id + i"
      class="row"
      :class="{ playing: current()?.id === t.id }"
      @click="PLAY_ON_SINGLE_CLICK && playAt(i)"
      @dblclick="playAt(i)"
      @contextmenu.prevent="openMenu($event, i)"
    >
      <button class="no" title="从这首开始播放" @click.stop="playAt(i)">
        <span class="n">{{ i + 1 }}</span><span class="p">▶</span>
      </button>
      <img v-if="t.cover" :src="t.cover" class="cover" loading="lazy" @click.stop="playAt(i)" />
      <div v-else class="cover ph" @click.stop="playAt(i)">♪</div>
      <div class="main">
        <div class="title">{{ t.title }}</div>
        <div class="sub">{{ t.artists.join(' / ') || '未知歌手' }}</div>
      </div>
      <div class="album">{{ t.album }}</div>
      <SourceBadge :source="t.source" />
      <div class="dur">{{ fmtTime(t.durationMs / 1000) }}</div>
    </div>
    <div v-if="!tracks.length" class="empty">暂无歌曲</div>

    <div v-if="menu.show" class="menu" :style="{ left: menu.x + 'px', top: menu.y + 'px' }">
      <div @click="playAt(menu.index); closeMenu()">从这首开始播放</div>
      <div @click="enqueue(tracks[menu.index]); closeMenu()">添加到播放队列末尾</div>
      <div @click="copyName(); closeMenu()">复制歌名</div>
    </div>
  </div>
</template>

<style scoped>
.row { display: grid; grid-template-columns: 36px 40px 1fr 30% 56px 48px; gap: 10px; align-items: center; padding: 6px 8px; border-radius: 8px; user-select: none; }
.row:hover { background: var(--hover); }
.row.playing .title { color: var(--accent); font-weight: 600; }
.no { background: none; border: 0; color: var(--muted); cursor: pointer; }
.no .p { display: none; }
.row:hover .no .n { display: none; }
.row:hover .no .p { display: inline; color: var(--accent); }
.cover { width: 40px; height: 40px; border-radius: 6px; object-fit: cover; cursor: pointer; }
.ph { display: grid; place-items: center; background: var(--hover); color: var(--muted); }
.title, .sub, .album { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.sub, .album, .dur { color: var(--muted); font-size: 13px; }
.empty { color: var(--muted); padding: 24px; text-align: center; }
.menu { position: fixed; background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 4px; box-shadow: 0 6px 20px rgba(0,0,0,.15); z-index: 10; }
.menu div { padding: 6px 14px; border-radius: 6px; cursor: pointer; }
.menu div:hover { background: var(--hover); }
</style>
