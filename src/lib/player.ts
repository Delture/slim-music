// 播放器：一个 <audio> + 队列。本地文件走 asset 协议，网易云按需取 URL。
// 队列 / 当前曲 / 进度写入 playback.json（防抖），启动时恢复。
// 网易云直链可能中途 stalled；waiting/stalled/error 超时后重取 URL 或切到已下完的缓存，并保持进度。
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

/** 当前桌面端资源是否走远程 URL（缓存未命中） */
let playingFromUrl = false;
let recoverAttempts = 0;
let recovering = false;
let stallWatchTimer: number | undefined;
let urlSwapTimer: number | undefined;
let lastTickTime = 0;
let lastTickAt = 0;
const MAX_RECOVER = 3;
const STALL_MS = 4000;
/** 当前曲剩余约这么多秒时预取下一首（落在 30–45s 窗口） */
const PREFETCH_REMAIN_SEC = 40;
let prefetchTargetId: number | null = null;
let prefetchInFlight = false;

/** 队列中「下一首」曲目（考虑列表循环；单曲循环不预取另一首） */
function peekNextTrack(): Track | null {
  if (!player.order.length || player.pos < 0) return null;
  if (player.repeat === 'one') return null;
  let nextPos = player.pos + 1;
  if (nextPos >= player.order.length) {
    if (player.repeat === 'off') return null;
    nextPos = 0;
  }
  const qi = player.order[nextPos];
  if (qi === undefined || qi === player.order[player.pos]) return null;
  return player.queue[qi] ?? null;
}

/**
 * 预取下一首网易云音频到磁盘缓存。
 * force=true：切歌/进队后立刻尝试；否则仅在剩余时长 ≤ PREFETCH_REMAIN_SEC 时触发。
 * 已缓存 / 已在飞 / 非网易云 → 跳过；invoke 立即返回，下载在 Rust 后台。
 */
function maybePrefetchNext(force = false) {
  const n = peekNextTrack();
  if (!n || n.source !== 'netease' || n.ncmId == null) return;
  if (prefetchInFlight && prefetchTargetId === n.ncmId) return;
  if (!force && prefetchTargetId === n.ncmId) return;
  if (!force) {
    const metaDur = (current()?.durationMs ?? 0) / 1000;
    const dur = player.duration > 0 ? player.duration : metaDur;
    if (!(dur > 0)) return;
    const remain = dur - player.currentTime;
    if (remain > PREFETCH_REMAIN_SEC || remain < 0) return;
  }
  prefetchTargetId = n.ncmId;
  prefetchInFlight = true;
  void api
    .prefetchSongAudio(n.ncmId)
    .catch(() => {
      if (prefetchTargetId === n.ncmId) prefetchTargetId = null;
    })
    .finally(() => {
      prefetchInFlight = false;
    });
}


function isAbortError(e: unknown): boolean {
  return (
    (typeof DOMException !== 'undefined' && e instanceof DOMException && e.name === 'AbortError') ||
    (!!e && typeof e === 'object' && 'name' in e && (e as { name: string }).name === 'AbortError')
  );
}

function clearStallWatch() {
  if (stallWatchTimer != null) {
    window.clearTimeout(stallWatchTimer);
    stallWatchTimer = undefined;
  }
}

function clearUrlSwap() {
  if (urlSwapTimer != null) {
    window.clearTimeout(urlSwapTimer);
    urlSwapTimer = undefined;
  }
}

function noteProgress() {
  lastTickTime = audio.currentTime;
  lastTickAt = Date.now();
}

function armStallWatch(delayMs = STALL_MS) {
  if (NATIVE) return;
  clearStallWatch();
  stallWatchTimer = window.setTimeout(() => {
    stallWatchTimer = undefined;
    void checkStallAndRecover();
  }, delayMs);
}

/** 下载完成后把远程直链热切换到本地缓存，降低中途断流概率 */
function scheduleUrlCacheSwap(token: number) {
  clearUrlSwap();
  if (NATIVE || !playingFromUrl) return;
  urlSwapTimer = window.setTimeout(() => {
    void (async () => {
      if (token !== loadToken || !playingFromUrl || recovering) return;
      const t = current();
      if (!t || t.source !== 'netease' || t.ncmId == null) return;
      try {
        const src = await api.songPlaySrc(t.ncmId, false);
        if (token !== loadToken || !src || src.kind !== 'file') {
          scheduleUrlCacheSwap(token);
          return;
        }
        const pos = audio.currentTime || player.currentTime || 0;
        const wantPlay = player.playing || !audio.paused;
        await applyDesktopSrc(src.value, true, wantPlay, pos, token);
        playingFromUrl = false;
        recoverAttempts = 0;
        noteProgress();
      } catch {
        scheduleUrlCacheSwap(token);
      }
    })();
  }, 2000);
}

async function checkStallAndRecover() {
  if (NATIVE || recovering) return;
  const t = current();
  if (!t) return;
  const wantPlay = player.playing || !audio.paused;
  if (!wantPlay) return;

  const now = Date.now();
  const noProgress = now - lastTickAt > STALL_MS - 500;
  const timeFrozen = Math.abs(audio.currentTime - lastTickTime) < 0.2;
  const networkWait = audio.readyState < 3; // HAVE_FUTURE_DATA
  const frozenPlaying = !audio.paused && noProgress && timeFrozen;
  const uiDesync = player.playing && audio.paused && noProgress;
  const waitingStuck = player.playing && networkWait && noProgress;

  if (!(frozenPlaying || uiDesync || waitingStuck)) return;

  if (recoverAttempts >= MAX_RECOVER) {
    player.playing = false;
    try {
      audio.pause();
    } catch {
      /* ignore */
    }
    player.error = `播放卡住：${t.title}`;
    return;
  }

  // 第一次优先切缓存/重取同曲；其后强制作废缓存
  const forceRefresh = recoverAttempts > 0 || !playingFromUrl;
  recoverAttempts += 1;
  await recoverPlayback(forceRefresh);
}

async function recoverPlayback(forceRefresh: boolean, seekTo?: number) {
  if (NATIVE || recovering) return;
  const t = current();
  if (!t) return;
  recovering = true;
  clearStallWatch();
  clearUrlSwap();
  const pos =
    seekTo != null && seekTo >= 0
      ? seekTo
      : audio.currentTime || player.currentTime || 0;
  try {
    await loadCurrent(true, pos, { forceRefresh });
  } finally {
    recovering = false;
  }
}

async function resolveSrc(
  t: Track,
  forceRefresh = false,
): Promise<{ src: string; fromUrl: boolean } | null> {
  if (NATIVE) {
    if (t.source === 'local' && t.path) return { src: t.path, fromUrl: false };
    if (t.source === 'netease' && t.ncmId != null) {
      const got = await api.songPlaySrc(t.ncmId, forceRefresh);
      if (!got) return null;
      if (got.kind === 'file') return { src: got.value, fromUrl: false };
      return { src: got.value.replace(/^http:\/\//i, 'https://'), fromUrl: true };
    }
    return null;
  }
  if (t.source === 'local' && t.path) {
    return { src: convertFileSrc(t.path), fromUrl: false };
  }
  if (t.source === 'netease' && t.ncmId != null) {
    const got = await api.songPlaySrc(t.ncmId, forceRefresh);
    if (!got) return null;
    if (got.kind === 'file') return { src: convertFileSrc(got.value), fromUrl: false };
    // 防御：后端已升 https，这里再兜一层，避免旧会话/异常路径残留 http
    return { src: got.value.replace(/^http:\/\//i, 'https://'), fromUrl: true };
  }
  return null;
}

let loadToken = 0;
let suppressPersist = false;

async function applyDesktopSrc(
  src: string,
  _isFile: boolean,
  autoplay: boolean,
  target: number,
  token: number,
) {
  audio.src = src;
  // 显式 load，避免沿用上一曲缓冲状态
  audio.load();
  if (target > 0) {
    await new Promise<void>((resolve) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        try {
          audio.currentTime = Math.min(target, audio.duration || target);
        } catch {
          /* ignore */
        }
        audio.removeEventListener('loadedmetadata', onMeta);
        window.clearTimeout(timer);
        resolve();
      };
      const onMeta = () => finish();
      const timer = window.setTimeout(finish, 8000);
      if (audio.readyState >= 1) finish();
      else audio.addEventListener('loadedmetadata', onMeta);
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
}

async function loadCurrent(
  autoplay = true,
  seekTo?: number,
  opts?: { forceRefresh?: boolean },
) {
  const t = current();
  if (!t) return;
  const token = ++loadToken;
  player.error = '';
  clearStallWatch();
  clearUrlSwap();
  if (!opts?.forceRefresh) recoverAttempts = 0;
  try {
    const resolved = await resolveSrc(t, !!opts?.forceRefresh);
    if (token !== loadToken) return;
    if (!resolved) {
      player.error = `无法播放「${t.title}」（无版权或需要 VIP），已跳过`;
      return next(true);
    }
    playingFromUrl = resolved.fromUrl;
    if (NATIVE) {
      const target = seekTo != null && seekTo > 0 ? seekTo : 0;
      await nativeCall('load', {
        url: resolved.src,
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
      maybePrefetchNext(true);
      return;
    }
    const target = seekTo != null && seekTo > 0 ? seekTo : 0;
    await applyDesktopSrc(resolved.src, !resolved.fromUrl, autoplay, target, token);
    if (token !== loadToken) return;
    noteProgress();
    if (playingFromUrl && autoplay) scheduleUrlCacheSwap(token);
    // 切歌/进队后立刻预取下一首（已缓存则跳过）
    maybePrefetchNext(true);
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
      if (isAbortError(e)) return;
      // 元素可能已卡死：重挂载到当前进度
      void recoverPlayback(playingFromUrl ? recoverAttempts > 0 : true);
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
  const wantPlay = player.playing || !audio.paused;
  try {
    audio.currentTime = sec;
  } catch {
    void recoverPlayback(true, sec);
    schedulePersist(400);
    return;
  }
  player.currentTime = sec;
  noteProgress();
  // 卡死后同曲 seek 常无效果：短暂观察，仍不到位则重挂载
  window.setTimeout(() => {
    if (recovering || loadToken === 0) return;
    if (!wantPlay && audio.paused) return;
    const missed = Math.abs(audio.currentTime - sec) > 1.25;
    const stuck = player.playing && (audio.paused || audio.readyState < 2);
    if (missed || stuck) {
      void recoverPlayback(true, sec);
    } else if (wantPlay && audio.paused) {
      void audio.play().catch(() => recoverPlayback(true, sec));
    }
  }, 700);
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
  noteProgress();
  schedulePersist(500);
});
audio.addEventListener('playing', () => {
  noteProgress();
  clearStallWatch();
  // 真正开播后重置计数，允许后续再遇 stall 时再恢复
  if (recoverAttempts > 0 && Date.now() - lastTickAt < 1000) {
    /* keep attempts until timeupdate proves progress */
  }
});
audio.addEventListener('pause', () => {
  player.playing = false;
  clearStallWatch();
  schedulePersist(300);
});
audio.addEventListener('timeupdate', () => {
  player.currentTime = audio.currentTime;
  noteProgress();
  if (recoverAttempts > 0) recoverAttempts = 0;
  maybePrefetchNext(false);
  const now = Date.now();
  // 进度约每 2s 落盘一次，避免狂写
  if (now - lastTimePersist > 2000) {
    lastTimePersist = now;
    schedulePersist(1500);
  }
});
audio.addEventListener('durationchange', () => (player.duration = audio.duration || 0));
audio.addEventListener('ended', () => next(true));
audio.addEventListener('waiting', () => {
  if (!player.playing && audio.paused) return;
  armStallWatch(STALL_MS);
});
audio.addEventListener('stalled', () => {
  if (!player.playing && audio.paused) return;
  armStallWatch(2500);
});
audio.addEventListener('emptied', () => {
  // 主动 load()/换源时也会 emptied，交给 loadToken 屏蔽；仅在播放意图下监视
  if (recovering) return;
  if (player.playing) armStallWatch(STALL_MS);
});
audio.addEventListener('error', () => {
  // 1 = MEDIA_ERR_ABORTED（切歌打断），忽略
  const code = audio.error?.code;
  if (code === 1) return;
  if (recovering) return;
  if (recoverAttempts < MAX_RECOVER) {
    recoverAttempts += 1;
    // 网络/解码错误：作废可能损坏的缓存并重取
    void recoverPlayback(true);
    return;
  }
  player.playing = false;
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
        maybePrefetchNext(false);
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
      case 'back':
        // 播放页打开时先关闭，否则退到后台（不销毁，播放继续）
        void import('./session').then(({ session, closeNowPlaying }) => {
          if (session.nowPlaying) closeNowPlaying();
          else void nativeCall('moveToBack');
        });
        break;
    }
  }).catch((e) => console.warn('native player listen failed', e));
} else if ('mediaSession' in navigator) {
  navigator.mediaSession.setActionHandler('play', togglePlay);
  navigator.mediaSession.setActionHandler('pause', togglePlay);
  navigator.mediaSession.setActionHandler('nexttrack', () => next());
  navigator.mediaSession.setActionHandler('previoustrack', prev);
}
