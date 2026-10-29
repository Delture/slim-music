<script setup lang="ts">
// 通用歌曲列表：双击行 / 单击封面或序号 / 右键「从这首开始播放」都会把当前整张列表设为队列并从该曲开始
import { reactive } from 'vue';
import type { Track } from '../lib/types';
import { current, enqueue, playList } from '../lib/player';
import { fmtTime } from '../lib/format';
import { isLiked, session, toggleLike } from '../lib/session';
import SourceBadge from './SourceBadge.vue';
import Icon from './Icon.vue';

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
async function onLike(t: Track, e: Event) {
  e.stopPropagation();
  if (t.ncmId == null || !session.profile) return;
  try {
    await toggleLike(t.ncmId);
  } catch (err) {
    alert(String(err));
  }
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
        <span class="n">{{ i + 1 }}</span><span class="p"><Icon name="play" :size="12" /></span>
      </button>
      <img v-if="t.cover" :src="t.cover" class="cover" loading="lazy" @click.stop="playAt(i)" />
      <div v-else class="cover ph" @click.stop="playAt(i)"><Icon name="music" :size="16" /></div>
      <div class="main">
        <div class="title">{{ t.title }}</div>
        <div class="sub">{{ t.artists.join(' / ') || '未知歌手' }}</div>
      </div>
      <div class="album">{{ t.album }}</div>
      <SourceBadge :source="t.source" />
      <button
        v-if="t.source === 'netease'"
        class="heart"
        :class="{ on: isLiked(t.ncmId), hide: !session.profile }"
        title="喜欢"
        @click="onLike(t, $event)"
      >
        <Icon :name="isLiked(t.ncmId) ? 'heart-solid' : 'heart'" :size="14" />
      </button>
      <span v-else class="heart spacer"></span>
      <div class="dur">{{ fmtTime(t.durationMs / 1000) }}</div>
    </div>
    <div v-if="!tracks.length" class="empty">暂无歌曲</div>

    <div v-if="menu.show" class="menu" :style="{ left: menu.x + 'px', top: menu.y + 'px' }">
      <div @click="playAt(menu.index); closeMenu()">从这首开始播放</div>
      <div @click="enqueue(tracks[menu.index]); closeMenu()">添加到播放队列末尾</div>
      <div
        v-if="tracks[menu.index]?.source === 'netease' && session.profile"
        @click="onLike(tracks[menu.index], $event); closeMenu()"
      >
        {{ isLiked(tracks[menu.index].ncmId) ? '取消喜欢' : '喜欢' }}
      </div>
      <div @click="copyName(); closeMenu()">复制歌名</div>
    </div>
  </div>
</template>

<style scoped>
.row {
  display: grid;
  grid-template-columns: 36px 40px 1fr 28% 56px 28px 48px;
  gap: 10px;
  align-items: center;
  padding: 6px 8px;
  border-radius: 8px;
  user-select: none;
}
.row:hover {
  background: var(--hover);
}
.row.playing .title {
  color: var(--accent);
  font-weight: 600;
}
.no {
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  line-height: 0;
}
.no .p {
  display: none;
  color: var(--accent);
}
.row:hover .no .n {
  display: none;
}
.row:hover .no .p {
  display: inline-flex;
}
.row:hover .no {
  background: var(--hover);
}
.cover {
  width: 40px;
  height: 40px;
  border-radius: 6px;
  object-fit: cover;
  cursor: pointer;
}
.ph {
  display: grid;
  place-items: center;
  background: var(--hover);
  color: var(--muted);
}
.title,
.sub,
.album {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.sub,
.album,
.dur {
  color: var(--muted);
  font-size: 13px;
}
.heart {
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  padding: 4px;
  line-height: 0;
  opacity: 0;
}
.row:hover .heart,
.heart.on {
  opacity: 1;
}
.heart.on {
  color: var(--accent);
}
.heart.hide {
  visibility: hidden;
}
.heart.spacer {
  visibility: hidden;
  pointer-events: none;
}
.empty {
  color: var(--muted);
  padding: 24px;
  text-align: center;
}
.menu {
  position: fixed;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 4px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.15);
  z-index: 10;
}
.menu div {
  padding: 6px 14px;
  border-radius: 6px;
  cursor: pointer;
}
.menu div:hover {
  background: var(--hover);
}
</style>
