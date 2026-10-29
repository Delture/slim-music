// 播放器：一个 <audio> + 队列。本地文件走 asset 协议，网易云按需取 URL。
import { reactive } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { api } from './api';
import type { Track } from './types';

export type Repeat = 'off' | 'all' | 'one';

const audio = new Audio();

export const player = reactive({
  queue: [] as Track[],
  /** 播放顺序：queue 的下标序列（随机时被打乱） */
  order: [] as number[],
  /** 在 order 中的位置 */
  pos: -1,
  playing: false,
  shuffle: false,
  repeat: 'off' as Repeat,
  currentTime: 0,
  duration: 0,
  volume: 0.8,
  error: '' as string,
});

export function current(): Track | null {
  const i = player.order[player.pos];
  return i === undefined ? null : player.queue[i] ?? null;
}

function buildOrder(startIndex: number) {
  const idx = player.queue.map((_, i) => i);
  if (!player.shuffle) {
    player.order = idx;
    player.pos = startIndex;
    return;
  }
  const rest = idx.filter(i => i !== startIndex);
  for (let i = rest.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [rest[i], rest[j]] = [rest[j], rest[i]];
  }
  player.order = [startIndex, ...rest];
  player.pos = 0;
}

async function resolveSrc(t: Track): Promise<string | null> {
  if (t.source === 'local' && t.path) return convertFileSrc(t.path);
  if (t.source === 'netease' && t.ncmId != null) {
    const src = await api.songPlaySrc(t.ncmId);
    if (!src) return null;
    if (src.kind === 'file') return convertFileSrc(src.value);
    return src.value;
  }
  return null;
}

let loadToken = 0;
async function loadCurrent(autoplay = true) {
  const t = current();
  if (!t) return;
  const token = ++loadToken;
  player.error = '';
  try {
    const src = await resolveSrc(t);
    if (token !== loadToken) return; // 用户已切到别的歌
    if (!src) {
      player.error = `无法播放「${t.title}」（无版权或需要 VIP），已跳过`;
      return next(true);
    }
    audio.src = src;
    if (autoplay) await audio.play();
  } catch (e) {
    player.error = String(e);
  }
}

/** 把整张列表设为队列，并从 startIndex 开始往后播放 */
export function playList(list: Track[], startIndex = 0) {
  player.queue = [...list];
  buildOrder(Math.max(0, Math.min(startIndex, list.length - 1)));
  void loadCurrent();
}

/** 追加到队列末尾（可混合本地与网易云歌曲） */
export function enqueue(t: Track) {
  player.queue.push(t);
  player.order.push(player.queue.length - 1);
  if (player.pos < 0) {
    player.pos = player.order.length - 1;
    void loadCurrent();
  }
}

export function togglePlay() {
  if (!audio.src) return void loadCurrent();
  audio.paused ? void audio.play() : audio.pause();
}

export function next(auto = false) {
  if (!player.order.length) return;
  if (auto && player.repeat === 'one') {
    audio.currentTime = 0;
    return void audio.play();
  }
  if (player.pos + 1 < player.order.length) player.pos++;
  else if (player.repeat !== 'off' || !auto) player.pos = 0;
  else return void (player.playing = false);
  void loadCurrent();
}

export function prev() {
  if (!player.order.length) return;
  if (audio.currentTime > 3) return void (audio.currentTime = 0);
  player.pos = player.pos > 0 ? player.pos - 1 : player.order.length - 1;
  void loadCurrent();
}

export function seek(sec: number) {
  audio.currentTime = sec;
}

export function setVolume(v: number, opts?: { persist?: boolean }) {
  const n = Math.min(1, Math.max(0, Number.isFinite(v) ? v : 0.8));
  player.volume = n;
  audio.volume = n;
  if (opts?.persist === false) return;
  void import('./session').then(({ session, persistPlaybackSettings }) => {
    session.settings.volume = n;
    persistPlaybackSettings();
  });
}

/** 启动时从 settings 恢复随机/循环（不写回磁盘） */
export function applyPlaybackPrefs(shuffle: boolean, repeat: Repeat) {
  player.shuffle = !!shuffle;
  player.repeat = repeat === 'all' || repeat === 'one' ? repeat : 'off';
}

export function toggleShuffle() {
  const cur = player.order[player.pos] ?? 0;
  player.shuffle = !player.shuffle;
  if (player.queue.length) buildOrder(cur);
  void import('./session').then(({ session, persistPlaybackSettings }) => {
    session.settings.shuffle = player.shuffle;
    persistPlaybackSettings();
  });
}

export function cycleRepeat() {
  player.repeat = player.repeat === 'off' ? 'all' : player.repeat === 'all' ? 'one' : 'off';
  void import('./session').then(({ session, persistPlaybackSettings }) => {
    session.settings.repeat = player.repeat;
    persistPlaybackSettings();
  });
}

audio.addEventListener('play', () => (player.playing = true));
audio.addEventListener('pause', () => (player.playing = false));
audio.addEventListener('timeupdate', () => (player.currentTime = audio.currentTime));
audio.addEventListener('durationchange', () => (player.duration = audio.duration || 0));
audio.addEventListener('ended', () => next(true));
audio.addEventListener('error', () => {
  player.error = `播放出错：${current()?.title ?? ''}`;
});
audio.volume = player.volume;

// TODO: 系统媒体键 / 正在播放信息（navigator.mediaSession 在 WebView 中部分可用）
if ('mediaSession' in navigator) {
  navigator.mediaSession.setActionHandler('play', togglePlay);
  navigator.mediaSession.setActionHandler('pause', togglePlay);
  navigator.mediaSession.setActionHandler('nexttrack', () => next());
  navigator.mediaSession.setActionHandler('previoustrack', prev);
}
