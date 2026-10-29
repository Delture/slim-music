<script setup lang="ts">
// 通用歌曲列表。播放：把当前列表设为「播放队列」从该曲开始（不影响本机播放列表内容）。
import { computed, reactive, ref } from 'vue';
import type { Track } from '../lib/types';
import { current, enqueue, playList } from '../lib/player';
import { fmtTime } from '../lib/format';
import { isLiked, session, toggleLike } from '../lib/session';
import {
  addTrackToLocal,
  localPl,
  refreshLocalPlaylists,
  removeTrackFromLocal,
} from '../lib/localPlaylists';
import SourceBadge from './SourceBadge.vue';
import Icon from './Icon.vue';

const PLAY_ON_SINGLE_CLICK = false;

const props = withDefaults(
  defineProps<{
    tracks: Track[];
    /** 当前若在某本机播放列表内，右键可「从本列表删除」 */
    localPlaylistId?: string | null;
    showReorder?: boolean;
  }>(),
  { localPlaylistId: null, showReorder: false },
);

const emit = defineEmits<{
  reorder: [{ from: number; to: number }];
}>();

const menu = reactive({
  show: false,
  x: 0,
  y: 0,
  index: -1,
  /** add | remove | null */
  sub: null as null | 'add' | 'remove',
});

const busy = ref(false);

const menuTrack = computed(() =>
  menu.index >= 0 ? props.tracks[menu.index] ?? null : null,
);

function playAt(i: number) {
  playList(props.tracks, i);
}
async function openMenu(e: MouseEvent, i: number) {
  if (!localPl.loaded) {
    try {
      await refreshLocalPlaylists();
    } catch {
      /* ignore */
    }
  }
  Object.assign(menu, { show: true, x: e.clientX, y: e.clientY, index: i, sub: null });
}
function closeMenu() {
  menu.show = false;
  menu.sub = null;
}
function copyName() {
  const t = menuTrack.value;
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

async function addTo(plId: string) {
  const t = menuTrack.value;
  if (!t || busy.value) return;
  busy.value = true;
  try {
    await addTrackToLocal(plId, t);
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
    closeMenu();
  }
}

async function removeFrom(plId: string, useIndex: boolean) {
  const t = menuTrack.value;
  if (!t || busy.value) return;
  busy.value = true;
  try {
    await removeTrackFromLocal(plId, t.id, useIndex ? menu.index : undefined);
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
    closeMenu();
  }
}

function move(i: number, dir: -1 | 1) {
  const to = i + dir;
  if (to < 0 || to >= props.tracks.length) return;
  emit('reorder', { from: i, to });
}
</script>

<template>
  <div class="list" @click="closeMenu">
    <div
      v-for="(t, i) in tracks"
      :key="t.id + '-' + i"
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
      <div v-if="showReorder" class="ord">
        <button :disabled="i === 0" title="上移" @click.stop="move(i, -1)">↑</button>
        <button :disabled="i === tracks.length - 1" title="下移" @click.stop="move(i, 1)">↓</button>
      </div>
    </div>
    <div v-if="!tracks.length" class="empty">暂无歌曲</div>

    <div
      v-if="menu.show"
      class="menu"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
      @click.stop
    >
      <template v-if="!menu.sub">
        <div @click="playAt(menu.index); closeMenu()">从这首开始播放</div>
        <div @click="enqueue(tracks[menu.index]); closeMenu()">添加到播放队列末尾</div>
        <div
          v-if="menuTrack?.source === 'netease' && session.profile"
          @click="onLike(menuTrack!, $event); closeMenu()"
        >
          {{ isLiked(menuTrack?.ncmId) ? '取消喜欢' : '喜欢' }}
        </div>
        <div class="has-sub" @click="menu.sub = 'add'">添加到播放列表 ▸</div>
        <div class="has-sub" @click="menu.sub = 'remove'">从播放列表删除 ▸</div>
        <div
          v-if="localPlaylistId"
          @click="removeFrom(localPlaylistId, true)"
        >
          从当前播放列表删除
        </div>
        <div @click="copyName(); closeMenu()">复制歌名</div>
      </template>
      <template v-else-if="menu.sub === 'add'">
        <div class="back" @click="menu.sub = null">◂ 返回</div>
        <div v-if="!localPl.lists.length" class="muted-item">还没有播放列表，请先在「播放列表」里新建</div>
        <div v-for="p in localPl.lists" :key="p.id" @click="addTo(p.id)">{{ p.name }}</div>
      </template>
      <template v-else>
        <div class="back" @click="menu.sub = null">◂ 返回</div>
        <div v-if="!localPl.lists.length" class="muted-item">还没有播放列表</div>
        <div v-for="p in localPl.lists" :key="p.id" @click="removeFrom(p.id, false)">
          {{ p.name }}
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.list {
  width: 100%;
  min-width: 0;
}
.row {
  display: grid;
  /* 固定列宽：主标题列 1fr + minmax(0) 才能正确 ellipsis */
  grid-template-columns: 36px 40px minmax(0, 1fr) minmax(0, 22%) 56px 28px 48px;
  gap: 10px;
  align-items: center;
  padding: 6px 8px;
  border-radius: 8px;
  user-select: none;
  min-width: 0;
}
.row:has(.ord) {
  grid-template-columns: 36px 40px minmax(0, 1fr) minmax(0, 20%) 56px 28px 48px 36px;
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
.main {
  min-width: 0;
  overflow: hidden;
}
.title,
.sub,
.album {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  min-width: 0;
}
.sub,
.album,
.dur {
  color: var(--muted);
  font-size: 13px;
}
.album {
  min-width: 0;
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
.dur {
  text-align: right;
  font-variant-numeric: tabular-nums;
}
.ord {
  display: flex;
  flex-direction: column;
  gap: 0;
}
.ord button {
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  padding: 0 4px;
  line-height: 1.1;
  font-size: 11px;
}
.ord button:disabled {
  opacity: 0.3;
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
  z-index: 30;
  min-width: 180px;
  max-height: 70vh;
  overflow: auto;
}
.menu div {
  padding: 6px 14px;
  border-radius: 6px;
  cursor: pointer;
}
.menu div:hover {
  background: var(--hover);
}
.menu .back {
  color: var(--muted);
  font-size: 13px;
}
.menu .muted-item {
  color: var(--muted);
  cursor: default;
  font-size: 13px;
}
.menu .muted-item:hover {
  background: transparent;
}
</style>
