// 播放器：一个 <audio> + 队列。本地文件走 asset 协议，网易云按需取 URL。
// 队列 / 当前曲 / 进度写入 playback.json（防抖），启动时恢复。
// 网易云：边播边后台下载；播放中不换源。卡住 / 回到前台时静默恢复。
// 播放栏错误只显示短中文，不把 ncm-api / OS 英文原文抛给用户。
import { reactive } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { api } from './api';
import { isAndroid, nativeCall, nativeListen } from './native';
import type { PlaybackSession, Track } from './types';
import { coverSrc } from './format';
import { playLog, playLogError } from './playLog';

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
/** 正在换源：忽略 load()/src 赋值触发的 pause，避免误跑 promote 打断首次播放 */
let swappingSrc = false;
/** 已成功挂到 audio / native 的曲目 id；与 current() 不一致时 togglePlay 应重载而非裸 play */
let loadedId: string | null = null;
/** loadCurrent 进行中：禁止 promote 用 autoplay:false 打断切歌 */
let loadInFlight = false;
/** 最近一次取源结果，供红字日志 */
let lastSrcKind: 'file' | 'url' | 'none' | 'error' = 'none';
let lastSrcError = '';
/** 曲末 ended / near-end error 去重，避免连跳两首 */
let lastEndAdvanceAt = 0;
let stallWatchTimer: number | undefined;
/** 用户连点上/下一首时合并为一次 load，避免并发取链 */
let skipLoadTimer: number | undefined;
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

/** 播放栏可见错误：只给需要用户处理的短中文；技术原文进 console。 */
function friendlyPlayError(e: unknown, title?: string | null): string {
  const raw = String(e ?? '').trim();
  if (raw) console.warn('[player]', raw);
  const lower = raw.toLowerCase();
  const tip = title?.trim() ? `「${title.trim()}」` : '';
  if (/authentication required|auth required|请先登录|未登录|301/.test(lower) || /登录/.test(raw) && /过期|失效|重新/.test(raw)) {
    return '登录已过期，请到「设置」重新扫码登录';
  }
  if (/无版权|需要 vip|需要vip|\bvip\b/.test(lower) || /copyright/i.test(raw)) {
    return tip ? `无法播放${tip}（无版权或需要 VIP）` : '无法播放（无版权或需要 VIP）';
  }
  if (/rate limit|rate limited|风控|限流|503/.test(lower)) {
    return '请求过于频繁，请稍后再试';
  }
  // ncm-api-rs: Invalid parameter / 取直链失败（连点切歌、直链过期等）
  if (
    /invalid parameter|unknown error|failed to fetch|networkerror|timeout|timed out|http request failed|api error|json error|crypto error/.test(
      lower,
    ) || /取直链失败/.test(raw)
  ) {
    return tip ? `暂时无法播放${tip}，请稍后重试` : '暂时无法播放，请稍后重试';
  }
  // 纯英文 / 技术堆栈 → 泛化
  if (!raw || /^[\x00-\x7F]+$/.test(raw)) {
    return tip ? `播放出错：${title!.trim()}` : '播放出错';
  }
  return raw.length > 60 ? (tip ? `播放出错：${title!.trim()}` : '播放出错') : raw;
}

function isTransientPlayError(e: unknown): boolean {
  const raw = String(e ?? '');
  const lower = raw.toLowerCase();
  return (
    /invalid parameter|unknown error|failed to fetch|networkerror|timeout|timed out|http request failed|api error|json error|crypto error/.test(
      lower,
    ) || /取直链失败/.test(raw)
  );
}

function snapshotCtx(extra?: Record<string, unknown>): Record<string, unknown> {
  const t = current();
  return {
    title: t?.title ?? null,
    id: t?.id ?? null,
    ncmId: t?.ncmId ?? null,
    pos: player.pos,
    queueLen: player.queue.length,
    playing: player.playing,
    loadedId,
    loadInFlight,
    swappingSrc,
    playingFromUrl,
    recoverAttempts,
    lastSrcKind,
    lastSrcError: lastSrcError || undefined,
    audioError: !NATIVE && audio.error ? audio.error.code : undefined,
    readyState: NATIVE ? undefined : audio.readyState,
    paused: NATIVE ? nat.paused : audio.paused,
    currentTime: Math.round((NATIVE ? nat.time : audio.currentTime) * 10) / 10,
    duration: Math.round((player.duration || 0) * 10) / 10,
    ...extra,
  };
}

function setVisibleError(msg: string, extra?: Record<string, unknown>) {
  player.error = msg;
  playLogError(msg, snapshotCtx(extra));
}

/** 曲自然结束或近曲末媒体错误：只推进一次 */
function advanceFromEnd() {
  const now = Date.now();
  if (now - lastEndAdvanceAt < 600) {
    playLog('end_dedupe', snapshotCtx({ sinceMs: now - lastEndAdvanceAt }));
    return;
  }
  lastEndAdvanceAt = now;
  playLog('advance_from_end', snapshotCtx());
  next(true);
}

function nearEnd(): boolean {
  const d = player.duration || audio.duration || 0;
  const t = audio.currentTime || player.currentTime || 0;
  return d > 0 && t >= d - 0.85;
}

function wantsPlayback(): boolean {
  return player.playing || !audio.paused;
}

function clearStallWatch() {
  if (stallWatchTimer != null) {
    window.clearTimeout(stallWatchTimer);
    stallWatchTimer = undefined;
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

/**
 * 暂停时静默切到已下完的本地缓存（不打断播放）。
 * 播放中绝不换源——那会造成开头可闻的卡顿。
 */
async function promoteToCacheIfPaused(token: number) {
  if (NATIVE || !playingFromUrl || recovering || swappingSrc || loadInFlight) return;
  // 曲末前后不要 promote，交给 ended / near-end 切歌
  if (nearEnd()) return;
  if (player.playing || !audio.paused) return;
  const t = current();
  if (!t || t.source !== 'netease' || t.ncmId == null) return;
  try {
    const src = await api.songPlaySrc(t.ncmId, false);
    if (token !== loadToken || !src || src.kind !== 'file') return;
    if (player.playing || !audio.paused) return;
    const pos = audio.currentTime || player.currentTime || 0;
    await applyDesktopSrc(convertFileSrc(src.value), true, false, pos, token);
    if (token !== loadToken) return;
    playingFromUrl = false;
    recoverAttempts = 0;
    noteProgress();
  } catch {
    /* 下次暂停再试 */
  }
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
    setVisibleError(`播放卡住：${t.title}`, { reason: 'stall_max' });
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
      try {
        const got = await api.songPlaySrc(t.ncmId, forceRefresh);
        if (!got) return null;
        if (got.kind === 'file') return { src: got.value, fromUrl: false };
        return { src: got.value.replace(/^http:\/\//i, 'https://'), fromUrl: true };
      } catch (e) {
        playLog('url_fail', {
          ncmId: t.ncmId,
          forceRefresh,
          err: String(e ?? '').slice(0, 360),
        });
        throw e;
      }
    }
    return null;
  }
  if (t.source === 'local' && t.path) {
    return { src: convertFileSrc(t.path), fromUrl: false };
  }
  if (t.source === 'netease' && t.ncmId != null) {
    try {
      const got = await api.songPlaySrc(t.ncmId, forceRefresh);
      if (!got) return null;
      if (got.kind === 'file') return { src: convertFileSrc(got.value), fromUrl: false };
      // 防御：后端已升 https，这里再兜一层，避免旧会话/异常路径残留 http
      return { src: got.value.replace(/^http:\/\//i, 'https://'), fromUrl: true };
    } catch (e) {
      // Rust 侧错误已含 id/level/各端点结果（无 cookie）
      playLog('url_fail', {
        ncmId: t.ncmId,
        forceRefresh,
        err: String(e ?? '').slice(0, 360),
      });
      throw e;
    }
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
  // 整个换源 + play/seek 期间都视为 swapping，避免 pause 回调误跑 promote
  swappingSrc = true;
  try {
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
  } finally {
    queueMicrotask(() => {
      swappingSrc = false;
    });
  }
}

async function loadCurrent(
  autoplay = true,
  seekTo?: number,
  opts?: { forceRefresh?: boolean; retried?: boolean },
) {
  const t = current();
  if (!t) return;
  if (skipLoadTimer != null) {
    window.clearTimeout(skipLoadTimer);
    skipLoadTimer = undefined;
  }
  const token = ++loadToken;
  player.error = '';
  clearStallWatch();
  loadInFlight = true;
  lastSrcKind = 'none';
  lastSrcError = '';
  if (!opts?.forceRefresh) recoverAttempts = 0;
  playLog('load', snapshotCtx({
    autoplay,
    seekTo: seekTo ?? 0,
    forceRefresh: !!opts?.forceRefresh,
    retried: !!opts?.retried,
  }));
  try {
    let resolved: { src: string; fromUrl: boolean } | null;
    try {
      resolved = await resolveSrc(t, !!opts?.forceRefresh);
    } catch (e) {
      lastSrcKind = 'error';
      lastSrcError = String(e ?? '');
      throw e;
    }
    if (token !== loadToken) return;
    if (!resolved) {
      loadedId = null;
      lastSrcKind = 'none';
      setVisibleError(`无法播放「${t.title}」（无版权或需要 VIP），已跳过`, {
        reason: 'no_url',
      });
      return next(true);
    }
    lastSrcKind = resolved.fromUrl ? 'url' : 'file';
    playingFromUrl = resolved.fromUrl;
    if (NATIVE) {
      const target = seekTo != null && seekTo > 0 ? seekTo : 0;
      await nativeCall('load', {
        url: resolved.src,
        id: t.id,
        title: t.title,
        artist: t.artists.join(' / '),
        album: t.album,
        artwork: !t.cover
          ? ''
          : /^(https?:|\/\/|content:)/i.test(t.cover)
            ? coverSrc(t.cover, '512y512')
            : 'file://' + t.cover,
        position: target,
        autoplay,
      });
      if (token !== loadToken) return;
      nat.hasSrc = true;
      nat.time = target;
      player.currentTime = target;
      if (t.durationMs) player.duration = t.durationMs / 1000;
      loadedId = t.id;
      if (!autoplay) player.playing = false;
      maybePrefetchNext(true);
      return;
    }
    const target = seekTo != null && seekTo > 0 ? seekTo : 0;
    await applyDesktopSrc(resolved.src, !resolved.fromUrl, autoplay, target, token);
    if (token !== loadToken) return;
    loadedId = t.id;
    noteProgress();
    // 切歌/进队后立刻预取下一首（已缓存则跳过）
    maybePrefetchNext(true);
  } catch (e) {
    if (token !== loadToken || isAbortError(e)) return;
    // 搜索 / 切歌取直链偶发 Invalid parameter：静默重试一次
    if (!opts?.retried && isTransientPlayError(e)) {
      playLog('load_retry', snapshotCtx({ err: String(e ?? '') }));
      await new Promise((r) => window.setTimeout(r, 220));
      if (token !== loadToken) return;
      return loadCurrent(autoplay, seekTo, {
        forceRefresh: true,
        retried: true,
      });
    }
    if (token === loadToken) {
      loadedId = null;
      player.playing = false;
      try {
        audio.pause();
      } catch {
        /* ignore */
      }
    }
    setVisibleError(friendlyPlayError(e, current()?.title), {
      reason: 'load_fail',
      raw: String(e ?? ''),
    });
  } finally {
    if (token === loadToken) {
      loadInFlight = false;
      schedulePersist(400);
    }
  }
}

/** 把整张列表设为队列，并从 startIndex 开始往后播放 */
export function playList(list: Track[], startIndex = 0) {
  player.queue = [...list];
  buildOrder(Math.max(0, Math.min(startIndex, list.length - 1)));
  loadedId = null; // 切队后旧 audio 源失效，等 loadCurrent 成功再对齐
  playLog('play_list', { len: list.length, startIndex, shuffle: player.shuffle });
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
  const cur = current();
  if (NATIVE) {
    if (!nat.hasSrc || !cur || loadedId !== cur.id) return void loadCurrent(true);
    void nativeCall(player.playing ? 'pause' : 'play').catch((e) => {
      setVisibleError(friendlyPlayError(e, current()?.title), { reason: 'native_toggle' });
    });
    schedulePersist(300);
    return;
  }
  // 媒体已坏 / 未对齐当前曲：必须重载（曲末 error 后裸 play 只会再失败）
  const mediaBroken = !!audio.error || audio.networkState === HTMLMediaElement.NETWORK_NO_SOURCE;
  if (!audio.src || !cur || loadedId !== cur.id || mediaBroken) {
    playLog('toggle_reload', snapshotCtx({ mediaBroken }));
    return void loadCurrent(true, undefined, { forceRefresh: mediaBroken });
  }
  if (audio.paused) {
    playLog('toggle_play', snapshotCtx());
    void audio.play().catch((e) => {
      if (isAbortError(e)) return;
      playLog('toggle_play_fail', snapshotCtx({ err: String(e ?? '') }));
      // 元素可能已卡死：强制重取源
      void recoverPlayback(true);
    });
  } else {
    playLog('toggle_pause', snapshotCtx());
    audio.pause();
  }
  schedulePersist(300);
}

/** 用户连点切歌：立刻作废在途 load，短暂合并后再取链 */
function scheduleUserSkipLoad() {
  loadToken++;
  loadInFlight = true;
  player.error = '';
  clearStallWatch();
  if (skipLoadTimer != null) window.clearTimeout(skipLoadTimer);
  skipLoadTimer = window.setTimeout(() => {
    skipLoadTimer = undefined;
    void loadCurrent(true);
  }, 140);
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
  // 曲末自动切歌立即加载；手动连点合并，减少 Invalid parameter
  if (auto) void loadCurrent();
  else scheduleUserSkipLoad();
}

export function prev() {
  if (!player.order.length) return;
  if (curTime() > 3) {
    seek(0);
    schedulePersist(300);
    return;
  }
  player.pos = player.pos > 0 ? player.pos - 1 : player.order.length - 1;
  scheduleUserSkipLoad();
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
  playLog('playing', snapshotCtx());
});
audio.addEventListener('playing', () => {
  player.error = '';
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
  // 换源 / 加载中 / 恢复中的 pause 不 promote
  if (swappingSrc || recovering || loadInFlight) return;
  playLog('pause', snapshotCtx({ nearEnd: nearEnd() }));
  // 暂停间隙切到本地缓存，恢复播放时不再依赖直链
  void promoteToCacheIfPaused(loadToken);
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
audio.addEventListener('ended', () => advanceFromEnd());
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
  if (recovering || swappingSrc || loadInFlight) {
    playLog('media_error_ignored', snapshotCtx({ code }));
    return;
  }
  playLog('media_error', snapshotCtx({ code, wants: wantsPlayback(), nearEnd: nearEnd() }));
  // 流式直链在曲末常抛 error 且不再触发 ended → 以前会静默停住，点播放再红字
  if (nearEnd()) {
    player.error = '';
    advanceFromEnd();
    return;
  }
  // 本就暂停（如后台挂起）：不弹红字、不强制打网易云
  if (!wantsPlayback()) {
    player.playing = false;
    player.error = '';
    return;
  }
  if (recoverAttempts < MAX_RECOVER) {
    recoverAttempts += 1;
    // 先尝试已下完的缓存；再失败才作废重取
    void recoverPlayback(recoverAttempts > 1 || !playingFromUrl);
    return;
  }
  player.playing = false;
  setVisibleError(friendlyPlayError('playback error', current()?.title), {
    reason: 'media_error_max',
    code,
  });
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
        advanceFromEnd();
        break;
      case 'error':
        console.warn(e.message);
        setVisibleError(friendlyPlayError(e.message || 'playback error', current()?.title), {
          reason: 'native_error',
        });
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

/** 回到前台：清掉过期红字；若仍在播且媒体已坏，静默重挂（不中途换源逻辑：仅 recover）。 */
function onAppForeground() {
  if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return;
  if (!player.error && !(audio.error && wantsPlayback())) return;
  if (!wantsPlayback() || nearEnd()) {
    player.error = '';
    return;
  }
  if (NATIVE) {
    player.error = '';
    return;
  }
  if (audio.error || audio.networkState === HTMLMediaElement.NETWORK_NO_SOURCE || audio.readyState === 0) {
    player.error = '';
    void recoverPlayback(playingFromUrl ? recoverAttempts > 0 : false);
  } else {
    player.error = '';
  }
}
if (typeof window !== 'undefined') {
  window.addEventListener('focus', onAppForeground);
  document.addEventListener('visibilitychange', onAppForeground);
}

