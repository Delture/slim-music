<script setup lang="ts">
import { dialog } from '../lib/dialog';
import { isMobile } from '../lib/native';
// 通用歌曲列表。播放：把当前列表设为「播放队列」从该曲开始（不影响本机播放列表内容）。
// 虚拟列表：只渲染视口附近行，消除长歌单上下滚动卡顿。
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue';
import { useVirtualizer } from '@tanstack/vue-virtual';
import { useAppMenuDismiss } from '../lib/appMenu';
import type { Track } from '../lib/types';
import { current, enqueue, playList } from '../lib/player';
import { fmtTime } from '../lib/format';
import CoverThumb from './CoverThumb.vue';
import { isLiked, session, toggleLike } from '../lib/session';
import {
  addTrackToLocal,
  localPl,
  refreshLocalPlaylists,
  removeTrackFromLocal,
} from '../lib/localPlaylists';
import SourceBadge from './SourceBadge.vue';
import Icon from './Icon.vue';

const PLAY_ON_SINGLE_CLICK = isMobile;

const props = withDefaults(
  defineProps<{
    tracks: Track[];
    /** 当前若在某本机播放列表内，右键可「从本列表删除」 */
    localPlaylistId?: string | null;
    showReorder?: boolean;
    /** 当前若为正在播放队列，右键可「从播放队列删除」 */
    queueEditable?: boolean;
  }>(),
  { localPlaylistId: null, showReorder: false, queueEditable: false },
);

const emit = defineEmits<{
  reorder: [{ from: number; to: number }];
  remove: [index: number];
}>();

const menu = reactive({
  show: false,
  x: 0,
  y: 0,
  index: -1,
  /** add | remove | null */
  sub: null as null | 'add' | 'remove',
});
const menuEl = ref<HTMLElement | null>(null);
const { openMenuAt, closeMenu: hideMenu } = useAppMenuDismiss(menu, menuEl);
function closeMenu() {
  hideMenu();
  menu.sub = null;
}

const busy = ref(false);
const rootEl = ref<HTMLElement | null>(null);
const scrollEl = ref<HTMLElement | null>(null);

const menuTrack = computed(() =>
  menu.index >= 0 ? props.tracks[menu.index] ?? null : null,
);

/** 只依赖当前曲 id，避免对每行调用 current() */
const playingId = computed(() => current()?.id ?? null);

/** 当前播放曲在本列表（筛选后）中的下标；不在列表里为 -1 */
const playingIndex = computed(() => {
  const id = playingId.value;
  if (id == null) return -1;
  const cur = current();
  if (cur) {
    const exact = props.tracks.indexOf(cur);
    if (exact >= 0) return exact;
  }
  return props.tracks.findIndex((tr) => tr.id === id);
});

/** 滚动视口高度，用于把定位按钮贴在可见区域右下角 */
const viewH = ref(0);
/** 当前播放行是否已完整落在可见区内（在的话隐藏按钮） */
const playingInView = ref(true);

const LOCATE_PAD = 72;

function rowStart(idx: number): number {
  const item = virtualizer.value.measurementsCache[idx];
  if (item) return item.start;
  return idx * estimateRowSize();
}

function updatePlayingInView() {
  const idx = playingIndex.value;
  const el = scrollEl.value;
  if (idx < 0 || !el || el.clientHeight <= 0) {
    playingInView.value = true;
    return;
  }
  const top = rowStart(idx);
  const size = estimateRowSize();
  const viewTop = el.scrollTop;
  const viewBot = viewTop + el.clientHeight;
  const marginTop = 8;
  const marginBot = 20;
  playingInView.value = top >= viewTop + marginTop && top + size <= viewBot - marginBot;
}

const showLocate = computed(
  () => playingIndex.value >= 0 && viewH.value > 0 && !playingInView.value,
);

function locatePlaying() {
  const idx = playingIndex.value;
  if (idx < 0) return;
  virtualizer.value.scrollToIndex(idx, { align: 'center', behavior: 'smooth' });
}

function findScrollParent(el: HTMLElement | null): HTMLElement | null {
  let p = el?.parentElement ?? null;
  while (p && p !== document.documentElement) {
    const { overflowY } = getComputedStyle(p);
    if (overflowY === 'auto' || overflowY === 'scroll' || overflowY === 'overlay') return p;
    p = p.parentElement;
  }
  return null;
}

function estimateRowSize() {
  /* cover + equal vertical padding — cover must not touch row bg edges */
  return window.matchMedia('(max-width: 700px)').matches ? 64 : 60;
}

const virtualizer = useVirtualizer(
  computed(() => ({
    count: props.tracks.length,
    getScrollElement: () => scrollEl.value,
    estimateSize: estimateRowSize,
    overscan: 12,
    paddingEnd: LOCATE_PAD,
    getItemKey: (i: number) => `${props.tracks[i]?.id ?? i}-${i}`,
  })),
);

const virtualRows = computed(() => virtualizer.value.getVirtualItems());
const totalSize = computed(() => virtualizer.value.getTotalSize());

let boundScroll: HTMLElement | null = null;
let scrollRo: ResizeObserver | null = null;

function onListScroll() {
  updatePlayingInView();
}

function bindScrollEl(el: HTMLElement | null) {
  if (boundScroll === el) return;
  if (boundScroll) boundScroll.removeEventListener('scroll', onListScroll);
  scrollRo?.disconnect();
  scrollRo = null;
  boundScroll = el;
  if (el) {
    el.addEventListener('scroll', onListScroll, { passive: true });
    if (typeof ResizeObserver !== 'undefined') {
      scrollRo = new ResizeObserver(() => {
        viewH.value = el.clientHeight;
        updatePlayingInView();
      });
      scrollRo.observe(el);
    }
  }
}

function refreshScrollParent() {
  scrollEl.value = findScrollParent(rootEl.value);
  viewH.value = scrollEl.value?.clientHeight ?? 0;
  bindScrollEl(scrollEl.value);
  virtualizer.value.measure();
  updatePlayingInView();
}

onMounted(() => {
  void nextTick(refreshScrollParent);
  window.addEventListener('resize', refreshScrollParent);
});
onUnmounted(() => {
  window.removeEventListener('resize', refreshScrollParent);
  bindScrollEl(null);
});

watch(
  () => props.tracks.length,
  () => void nextTick(() => {
    virtualizer.value.measure();
    refreshScrollParent();
  }),
);
watch(playingIndex, () => void nextTick(updatePlayingInView));

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
  menu.index = i;
  menu.sub = null;
  openMenuAt(e);
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
    void dialog.alert(String(err));
  }
}

async function addTo(plId: string) {
  const t = menuTrack.value;
  if (!t || busy.value) return;
  busy.value = true;
  try {
    await addTrackToLocal(plId, t);
  } catch (e) {
    void dialog.alert(String(e));
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
    void dialog.alert(String(e));
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
  <div ref="rootEl" class="list">
    <div
      class="locate-slot"
      :style="{ '--locate-view-h': viewH + 'px' }"
    >
      <Transition name="locate-in">
        <button
          v-if="showLocate"
          key="locate"
          type="button"
          class="locate"
          title="定位到当前播放"
          aria-label="定位到当前播放"
          @click="locatePlaying"
        >
          <Icon name="locate" :size="18" />
        </button>
      </Transition>
    </div>
    <div v-if="tracks.length" class="virt" :style="{ height: totalSize + 'px' }">
      <div
        v-for="vRow in virtualRows"
        :key="String(vRow.key)"
        class="row"
        :class="{ playing: playingId === tracks[vRow.index]?.id }"
        :data-index="vRow.index"
        :style="{
          position: 'absolute',
          top: 0,
          left: 0,
          width: '100%',
          height: vRow.size + 'px',
          transform: `translateY(${vRow.start}px)`,
        }"
        @click="PLAY_ON_SINGLE_CLICK && playAt(vRow.index)"
        @dblclick="playAt(vRow.index)"
        @contextmenu.prevent="openMenu($event, vRow.index)"
      >
        <button class="no" title="从这首开始播放" @click.stop="playAt(vRow.index)">
          <span class="n">{{ vRow.index + 1 }}</span
          ><span class="p"><Icon name="play" :size="12" /></span>
        </button>
        <CoverThumb
          :src="tracks[vRow.index]?.cover"
          @click="playAt(vRow.index)"
        />
        <div class="main">
          <div class="title">{{ tracks[vRow.index].title }}</div>
          <div class="sub">{{ tracks[vRow.index].artists.join(' / ') || '未知歌手' }}</div>
        </div>
        <div class="album">{{ tracks[vRow.index].album }}</div>
        <SourceBadge :source="tracks[vRow.index].source" />
        <button
          v-if="tracks[vRow.index].source === 'netease'"
          class="heart"
          :class="{ on: isLiked(tracks[vRow.index].ncmId), hide: !session.profile }"
          title="喜欢"
          @click="onLike(tracks[vRow.index], $event)"
        >
          <Icon :name="isLiked(tracks[vRow.index].ncmId) ? 'heart-solid' : 'heart'" :size="14" />
        </button>
        <span v-else class="heart spacer"></span>
        <div class="dur">{{ fmtTime(tracks[vRow.index].durationMs / 1000) }}</div>
        <div v-if="showReorder" class="ord">
          <button
            type="button"
            :disabled="vRow.index === 0"
            title="上移"
            @click.stop="move(vRow.index, -1)"
          >
            <Icon name="chevron-up" :size="12" />
          </button>
          <button
            type="button"
            :disabled="vRow.index === tracks.length - 1"
            title="下移"
            @click.stop="move(vRow.index, 1)"
          >
            <Icon name="chevron-down" :size="12" />
          </button>
        </div>
      </div>
    </div>
    <div v-else class="empty">暂无歌曲</div>

    <Teleport to="body">
    <div
      v-if="menu.show"
      ref="menuEl"
      class="menu"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
      @click.stop
      @pointerdown.stop
    >
      <template v-if="!menu.sub">
        <div @click="playAt(menu.index); closeMenu()">从这首开始播放</div>
        <div @click="enqueue(tracks[menu.index]); closeMenu()">添加到播放队列末尾</div>
        <div
          v-if="queueEditable"
          @click="emit('remove', menu.index); closeMenu()"
        >从播放队列删除</div>
        <div
          v-if="menuTrack?.source === 'netease' && session.profile"
          @click="onLike(menuTrack!, $event); closeMenu()"
        >
          {{ isLiked(menuTrack?.ncmId) ? '取消喜欢' : '喜欢' }}
        </div>
        <div class="has-sub" @click="menu.sub = 'add'">添加到播放列表 ▸</div>
        <div class="has-sub" @click="menu.sub = 'remove'">从播放列表删除 ▸</div>
        <div v-if="localPlaylistId" @click="removeFrom(localPlaylistId, true)">从当前播放列表删除</div>
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
    </Teleport>
  </div>
</template>

<style scoped>
.list {
  width: 100%;
  min-width: 0;
  position: relative;
}
.locate-slot {
  --locate-size: 40px;
  --locate-inset: 16px;
  position: sticky;
  top: 0;
  height: 0;
  z-index: 6;
  pointer-events: none;
  display: flex;
  justify-content: flex-end;
  overflow: visible;
}
.locate {
  pointer-events: auto;
  position: relative;
  top: calc(var(--locate-view-h) - var(--locate-inset) - var(--locate-size));
  margin-right: var(--locate-inset);
  width: var(--locate-size);
  height: var(--locate-size);
  min-width: var(--locate-size);
  min-height: var(--locate-size);
  padding: 0;
  border-radius: 50%;
  border: 0.5px solid var(--border);
  background: color-mix(in srgb, var(--panel-bg) 88%, transparent);
  box-shadow: var(--elev-2);
  color: var(--text-secondary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  line-height: 0;
  transition:
    background var(--dur-fast) var(--ease-swift),
    color var(--dur-fast) var(--ease-swift),
    box-shadow var(--dur-fast) var(--ease-swift),
    transform var(--dur-fast) var(--ease-swift),
    opacity var(--dur-fast) var(--ease-swift);
}
.locate:hover {
  color: var(--accent);
  background: var(--panel-bg);
  box-shadow: var(--elev-2);
}
.locate:active {
  transform: scale(0.96);
}
.locate-in-enter-active,
.locate-in-leave-active {
  transition:
    opacity var(--dur) var(--ease-swift),
    transform var(--dur) var(--ease-swift);
}
.locate-in-enter-from,
.locate-in-leave-to {
  opacity: 0;
  transform: scale(0.92);
}
@media (max-width: 700px) {
  .locate-slot {
    --locate-size: 44px;
    --locate-inset: 12px;
  }
}
.virt {
  width: 100%;
  position: relative;
  contain: layout style;
}
.row {
  display: grid;
  grid-template-columns: 40px 44px minmax(0, 1fr) minmax(0, 22%) 56px 32px 52px;
  gap: var(--row-gap);
  align-items: center;
  /* Equal top/bottom padding so cover is optically centered and never clips row bg */
  padding: var(--space-3) var(--row-px);
  border-radius: var(--row-radius);
  user-select: none;
  min-width: 0;
  box-sizing: border-box;
  content-visibility: auto;
  contain-intrinsic-size: auto 60px;
  /* Avoid paint storms while scrolling long lists */
  will-change: transform;
  transition:
    background var(--dur-fast) var(--ease-swift),
    color var(--dur-fast) var(--ease-swift),
    transform var(--dur-fast) var(--ease-swift);
}
.row:has(.ord) {
  grid-template-columns: 40px 48px minmax(0, 1fr) minmax(0, 20%) 56px 32px 52px 22px;
  padding-right: calc(var(--row-px) + var(--scrollbar-size, 10px));
}
.row:hover {
  background: var(--fill-secondary);
}
.row.playing {
  background: var(--selected);
  box-shadow: none;
}
.row.playing .title {
  color: var(--text);
  font-weight: 650;
  transition: color var(--dur-fast) var(--ease-swift), font-weight var(--dur-fast) var(--ease-swift);
}
.no {
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  width: 32px;
  height: 32px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius);
  line-height: 0;
  font-variant-numeric: tabular-nums;
  font-size: 13px;
  opacity: 0.7;
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
  opacity: 1;
}
/* cover via CoverThumb */
.main {
  min-width: 0;
  overflow: hidden;
}
.title {
  font-size: 15px;
  font-weight: 600;
  letter-spacing: -0.015em;
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
  color: var(--text-secondary);
  font-size: 13px;
  opacity: 0.85;
}
.sub {
  margin-top: 2px;
  font-size: 12px;
}
.album {
  min-width: 0;
  font-size: 14px;
  opacity: 0.78;
}
.heart {
  background: none;
  border: 0;
  color: var(--muted);
  cursor: pointer;
  padding: 0;
  line-height: 0;
  opacity: 0;
  border-radius: 50%;
  width: var(--icon-btn);
  min-width: var(--icon-btn);
  height: var(--icon-btn);
  min-height: var(--icon-btn);
  transition:
    opacity var(--dur-fast) var(--ease-swift),
    color var(--dur-fast) var(--ease-swift),
    background var(--dur-fast) var(--ease-swift);
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
  font-size: 13px;
}
.ord {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0;
  width: 100%;
}
.ord button {
  background: none;
  border: 0;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 0;
  margin: 0;
  line-height: 0;
  width: 22px;
  height: 16px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius);
  opacity: 0.8;
}
.ord button :deep(.icon) {
  vertical-align: 0;
}
.ord button:hover:not(:disabled) {
  color: var(--text);
  background: var(--hover);
  opacity: 1;
}
.ord button:disabled {
  opacity: 0.28;
  cursor: default;
}
.empty {
  color: var(--muted);
  padding: var(--empty-pad);
  text-align: center;
  font-size: 15px;
}
.menu {
  position: fixed;
  background: var(--surface-3);
  border: 0.5px solid var(--border);
  border-radius: var(--radius-lg);
  padding: var(--menu-pad);
  box-shadow: var(--elev-2);
  z-index: 9000;
  min-width: 200px;
  max-height: 70vh;
  overflow: auto;
  animation: sm-scale-in var(--dur) var(--ease-enter);
  transform-origin: top left;
  /* Solid menu — backdrop-filter while list scrolls is a major jank source */
  -webkit-backdrop-filter: none;
  backdrop-filter: none;
}
.menu div {
  padding: var(--menu-item-py) var(--menu-item-px);
  border-radius: var(--radius);
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
  transition: background var(--dur-fast) var(--ease-swift);
}
.menu div:hover {
  background: var(--fill-secondary);
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

@media (max-width: 700px) {
  .row {
    grid-template-columns: 48px minmax(0, 1fr) 44px;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-1);
    contain-intrinsic-size: auto 64px;
  }
  .row .no,
  .row .album,
  .row .dur,
  .row > .badge {
    display: none;
  }
  .row:has(.ord) {
    grid-template-columns: 48px minmax(0, 1fr) 44px 22px;
    padding-right: calc(var(--space-1) + var(--scrollbar-size, 10px));
  }
  :deep(.cover-thumb) {
    width: 44px;
    height: 44px;
  }
  .heart {
    min-width: 44px;
    min-height: 44px;
    opacity: 1;
  }
  .title {
    font-size: 16px;
  }
}
</style>
