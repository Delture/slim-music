// 播放器：一个 <audio> + 队列。本地文件走 asset 协议，网易云按需取 URL。
// 队列 / 当前曲 / 进度写入 playback.json（防抖），启动时恢复。
import { reactive } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { api } from './api';
import { isAndroid, nativeCall, nativeListen } from './native';
import type { PlaybackSession, Track } from './types';

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
  const rest = idx.filter((i) => i !== startIndex);
  for (let i = rest.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [rest[i], rest[j]] = [rest[j], rest[i]];
  }
  player.order = [startIndex, ...rest];
  player.pos = 0;
}

/** Android：后台播放走原生 Media3 服务（WebView <audio> 熄屏后会被挂起） */
const NATIVE = isAndroid;
/** 原生端最近一次状态 */
const nat = { hasSrc: false, paused: true, time: 0 };

async function resolveSrc(t: Track): Promise<string | null> {
  if (NATIVE) {
    if (t.source === 'local' && t.path) return t.path; // content:// 或绝对路径
    if (t.source === 'netease' && t.ncmId != null) {
      const src = await api.songPlaySrc(t.ncmId);
      if (!src) return null;
      if (src.kind === 'file') return src.value;
      return src.value.replace(/^http:\/\//i, 'https://');
    }
    return null;
  }
  if (t.source === 'local' && t.path) return convertFileSrc(t.path);
  if (t.source === 'netease' && t.ncmId != null) {
    const src = await api.songPlaySrc(t.ncmId);
    if (!src) return null;
    if (src.kind === 'file') return convertFileSrc(src.value);
    // 防御：后端已升 https，这里再兜一层，避免旧会话/异常路径残留 http
    return src.value.replace(/^http:\/\//i, 'https://');
  }
  return null;
}

let loadToken = 0;
let suppressPersist = false;

function isAbortError(e: unknown): boolean {
  return (
    (typeof DOMException !== 'undefined' && e instanceof DOMException && e.name === 'AbortError') ||
    (!!e && typeof e === 'object' && 'name' in e && (e as { name: string }).name === 'AbortError')
  );
}

async function loadCurrent(autoplay = true, seekTo?: number) {
  const t = current();
  if (!t) return;
  const token = ++loadToken;
  player.error = '';
  try {
    const src = await resolveSrc(t);
    if (token !== loadToken) return;
    if (!src) {
      player.error = `无法播放「${t.title}」（无版权或需要 VIP），已跳过`;
      return next(true);
    }
    if (NATIVE) {
      const target = seekTo != null && seekTo > 0 ? seekTo : 0;
      await nativeCall('load', {
        url: src,
        id: t.id,
        title: t.title,
        artist: t.artists.join(' / '),
        album: t.album,
        artwork: t.cover ? t.cover.replace(/^http:\/\//i, 'https://') + '?param=512y512' : '',
        position: target,
        autoplay,
      });
      if (token !== loadToken) return;
      nat.hasSrc = true;
      nat.time = target;
      player.currentTime = target;
      if (t.durationMs) player.duration = t.durationMs / 1000;
      if (!autoplay) player.playing = false;
      return;
    }
    audio.src = src;
    // 显式 load，避免沿用上一曲缓冲状态
    audio.load();
    const target = seekTo != null && seekTo > 0 ? seekTo : 0;
    if (target > 0) {
      await new Promise<void>((resolve) => {
        const done = () => {
          try {
            audio.currentTime = Math.min(target, audio.duration || target);
          } catch {
            /* ignore */
          }
          audio.removeEventListener('loadedmetadata', done);
          resolve();
        };
        if (audio.readyState >= 1) done();
        else audio.addEventListener('loadedmetadata', done);
      });
      if (token !== loadToken) return;
      player.currentTime = audio.currentTime;
    }
    if (autoplay) {
      try {
        await audio.play();
      } catch (e) {
        // 切歌时前一次 play() 被 abort 是正常的，不当错误
        if (token !== loadToken || isAbortError(e)) return;
        throw e;
      }
    } else {
      // 预加载到可 seek，界面显示曲目与进度，等用户按播放
      player.playing = false;
    }
  } catch (e) {
    if (token !== loadToken || isAbortError(e)) return;
    player.error = String(e);
  } finally {
    if (token === loadToken) schedulePersist(400);
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
  } else {
    schedulePersist(400);
  }
}

export function togglePlay() {
  if (NATIVE) {
    if (!nat.hasSrc) return void loadCurrent(true);
    void nativeCall(player.playing ? 'pause' : 'play').catch((e) => (player.error = String(e)));
    schedulePersist(300);
    return;
  }
  if (!audio.src) return void loadCurrent(true);
  if (audio.paused) {
    void audio.play().catch((e) => {
      if (!isAbortError(e)) player.error = String(e);
    });
  } else audio.pause();
  schedulePersist(300);
}

export function next(auto = false) {
  if (!player.order.length) return;
  if (auto && player.repeat === 'one') {
    if (NATIVE) {
      void nativeCall('seek', { position: 0 }).then(() => nativeCall('play'));
      return;
    }
    audio.currentTime = 0;
    return void audio.play().catch(() => {});
  }
  if (player.pos + 1 < player.order.length) player.pos++;
  else if (player.repeat !== 'off' || !auto) player.pos = 0;
  else {
    player.playing = false;
    schedulePersist(300);
    return;
  }
  void loadCurrent();
}

export function prev() {
  if (!player.order.length) return;
  if (curTime() > 3) {
    seek(0);
    schedulePersist(300);
    return;
  }
  player.pos = player.pos > 0 ? player.pos - 1 : player.order.length - 1;
  void loadCurrent();
}

function curTime(): number {
  return NATIVE ? nat.time : audio.currentTime;
}

export function seek(sec: number) {
  if (NATIVE) {
    nat.time = sec;
    player.currentTime = sec;
    void nativeCall('seek', { position: sec });
    schedulePersist(400);
    return;
  }
  audio.currentTime = sec;
  player.currentTime = sec;
  schedulePersist(400);
}

export function setVolume(v: number, opts?: { persist?: boolean }) {
  const n = Math.min(1, Math.max(0, Number.isFinite(v) ? v : 0.8));
  player.volume = n;
  audio.volume = n;
  if (NATIVE) void nativeCall('setVolume', { volume: n }).catch(() => {});
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
  schedulePersist(400);
}

export function cycleRepeat() {
  player.repeat = player.repeat === 'off' ? 'all' : player.repeat === 'all' ? 'one' : 'off';
  void import('./session').then(({ session, persistPlaybackSettings }) => {
    session.settings.repeat = player.repeat;
    persistPlaybackSettings();
  });
}

function snapshot(): PlaybackSession {
  return {
    queue: player.queue.slice(0, 500),
    order: [...player.order],
    pos: player.pos,
    currentTime: curTime() || player.currentTime || 0,
    wasPlaying: false,
  };
}

let persistTimer: number | undefined;
let lastTimePersist = 0;

export function schedulePersist(delayMs = 800) {
  if (suppressPersist) return;
  if (persistTimer) window.clearTimeout(persistTimer);
  persistTimer = window.setTimeout(() => {
    void api.savePlaybackSession(snapshot()).catch(() => {});
  }, delayMs);
}

/** 从 playback.json 恢复队列与进度；一律暂停在已存进度，不自动播放 */
export async function restorePlaybackSession() {
  suppressPersist = true;
  try {
    const snap = await api.getPlaybackSession();
    if (!snap?.queue?.length) return;
    player.queue = snap.queue;
    const order = (snap.order ?? []).filter((i) => i >= 0 && i < player.queue.length);
    if (order.length === player.queue.length) {
      player.order = order;
      player.pos =
        snap.pos >= 0 && snap.pos < order.length ? snap.pos : 0;
    } else {
      buildOrder(Math.max(0, Math.min(snap.pos ?? 0, player.queue.length - 1)));
    }
    const seekTo = Math.max(0, Number(snap.currentTime) || 0);
    await loadCurrent(false, seekTo);
  } catch (e) {
    console.warn('restore playback failed', e);
  } finally {
    suppressPersist = false;
    schedulePersist(1000);
  }
}

audio.addEventListener('play', () => {
  player.playing = true;
  player.error = '';
  schedulePersist(500);
});
audio.addEventListener('pause', () => {
  player.playing = false;
  schedulePersist(300);
});
audio.addEventListener('timeupdate', () => {
  player.currentTime = audio.currentTime;
  const now = Date.now();
  // 进度约每 2s 落盘一次，避免狂写
  if (now - lastTimePersist > 2000) {
    lastTimePersist = now;
    schedulePersist(1500);
  }
});
audio.addEventListener('durationchange', () => (player.duration = audio.duration || 0));
audio.addEventListener('ended', () => next(true));
audio.addEventListener('error', () => {
  // 1 = MEDIA_ERR_ABORTED（切歌打断），忽略
  const code = audio.error?.code;
  if (code === 1) return;
  player.error = `播放出错：${current()?.title ?? ''}`;
});
audio.volume = player.volume;

if (NATIVE) {
  let lastPlaying = false;
  void nativeListen((e) => {
    switch (e.type) {
      case 'state':
        nat.time = e.position;
        nat.paused = !e.playWhenReady;
        player.currentTime = e.position;
        if (e.duration > 0) player.duration = e.duration;
        // 缓冲中仍视为播放态，避免按钮闪烁
        player.playing = e.playing || (e.playWhenReady && e.buffering);
        if (player.playing !== lastPlaying) {
          lastPlaying = player.playing;
          if (player.playing) player.error = '';
          schedulePersist(400);
        } else {
          const now = Date.now();
          if (now - lastTimePersist > 2000) {
            lastTimePersist = now;
            schedulePersist(1500);
          }
        }
        break;
      case 'ended':
        next(true);
        break;
      case 'error':
        player.error = `播放出错：${current()?.title ?? ''}`;
        console.warn(e.message);
        break;
      case 'remote':
        if (e.action === 'next') next();
        else prev();
        break;
    }
  }).catch((e) => console.warn('native player listen failed', e));
} else if ('mediaSession' in navigator) {
  navigator.mediaSession.setActionHandler('play', togglePlay);
  navigator.mediaSession.setActionHandler('pause', togglePlay);
  navigator.mediaSession.setActionHandler('nexttrack', () => next());
  navigator.mediaSession.setActionHandler('previoustrack', prev);
}
