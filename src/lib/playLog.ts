// 轻量播放诊断日志：内存环形缓冲 + 落盘到 app_data/play-log.txt。
// 出现播放栏红字时自动写入上下文；不自动上传，由用户复制/导出。
import { invoke } from '@tauri-apps/api/core';

const RING = 240;
const lines: string[] = [];
let flushTimer: number | undefined;
let pendingDisk: string[] = [];

function ts(): string {
  const d = new Date();
  const p = (n: number, w = 2) => String(n).padStart(w, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}

function pushLine(line: string) {
  lines.push(line);
  if (lines.length > RING) lines.splice(0, lines.length - RING);
  pendingDisk.push(line);
  if (pendingDisk.length > 80) pendingDisk.splice(0, pendingDisk.length - 80);
}

/** 记录一条播放事件（短字符串 + 可选结构化字段） */
export function playLog(event: string, data?: Record<string, unknown>) {
  let extra = '';
  if (data && Object.keys(data).length) {
    try {
      extra = ' ' + JSON.stringify(data);
    } catch {
      extra = ' [unserializable]';
    }
    if (extra.length > 600) extra = extra.slice(0, 600) + '…';
  }
  pushLine(`${ts()} [${event}]${extra}`);
  scheduleFlush(false);
}

/** 红字错误：立刻附带上下文并刷盘 */
export function playLogError(message: string, ctx?: Record<string, unknown>) {
  playLog('ui_error', { message, ...ctx });
  scheduleFlush(true);
}

export function getPlayLogText(): string {
  return lines.join('\n');
}

export function clearPlayLogMemory() {
  lines.length = 0;
  pendingDisk.length = 0;
}

function scheduleFlush(immediate: boolean) {
  if (flushTimer != null) {
    window.clearTimeout(flushTimer);
    flushTimer = undefined;
  }
  const run = () => {
    flushTimer = undefined;
    void flushToDisk();
  };
  if (immediate) run();
  else flushTimer = window.setTimeout(run, 800);
}

async function flushToDisk() {
  if (!pendingDisk.length) return;
  const batch = pendingDisk.join('\n') + '\n';
  pendingDisk = [];
  try {
    await invoke('append_play_log', { text: batch });
  } catch {
    /* 落盘失败不影响播放 */
  }
}

/** 合并内存 + 磁盘日志，便于用户复制 */
export async function exportPlayLogText(): Promise<string> {
  await flushToDisk();
  let disk = '';
  try {
    disk = await invoke<string>('read_play_log');
  } catch {
    disk = '';
  }
  const mem = getPlayLogText();
  if (!disk) return mem;
  if (!mem) return disk;
  // 磁盘可能已含近期行；简单拼接并去重尾部
  return `${disk.trimEnd()}\n--- memory ring ---\n${mem}`;
}

export async function clearPlayLogAll(): Promise<void> {
  clearPlayLogMemory();
  try {
    await invoke('clear_play_log');
  } catch {
    /* ignore */
  }
}

export async function copyPlayLog(): Promise<boolean> {
  const text = await exportPlayLogText();
  if (!text.trim()) return false;
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
