// 托盘：右侧图标仅控制。歌词走刘海左侧透明条（主窗口隐藏时才显示）。
import { listen } from '@tauri-apps/api/event';
import { api } from './api';
import { current, next, player, prev, togglePlay } from './player';
import type { LyricLine } from './types';

let lines: LyricLine[] = [];
let lastTrackKey = '';
let lastEmit = '';
let mainVisible = true;
let lyricToken = 0;

export function setTrayLyricLines(l: LyricLine[]) {
  lines = l;
  lastTrackKey = current()?.id ?? lastTrackKey;
  lastEmit = '';
  pushLyric();
}

export function clearTrayLyric() {
  lines = [];
  lastEmit = '';
  pushLyric();
}

function activeLine(ms: number): string {
  if (!lines.length) return '';
  let hit: LyricLine | null = null;
  for (const l of lines) {
    if (l.timeMs <= ms) hit = l;
    else break;
  }
  if (!hit) return '';
  const orig = hit.text.trim();
  if (!orig) return '';
  const tr = (hit.translation ?? '').trim();
  if (!tr || tr === orig) return orig;
  // 菜单栏空间紧：原文为主，短翻译用间隔点缀
  const shortTr = [...tr].length > 12 ? `${[...tr].slice(0, 11).join('')}…` : tr;
  return `${orig} · ${shortTr}`;
}

function pushLyric() {
  const text = mainVisible ? '' : activeLine(player.currentTime * 1000);
  if (text === lastEmit) return;
  lastEmit = text;
  void api.setMenubarLyric(text).catch(() => {});
}

async function loadLyricsForCurrent() {
  const t = current();
  const key = t?.id ?? '';
  if (key === lastTrackKey) return;
  lastTrackKey = key;
  lastEmit = '';
  lines = [];
  void api.setMenubarLyric('').catch(() => {});
  void api.setTrayTitle('').catch(() => {});

  if (!t || t.source !== 'netease' || t.ncmId == null) return;

  const token = ++lyricToken;
  try {
    const payload = await api.lyric(t.ncmId);
    if (token !== lyricToken) return;
    // 若 NowPlaying 已写入更新的行，不要覆盖
    if ((current()?.id ?? '') !== key) return;
    lines = payload.lines.filter((l) => l.text.trim());
    lastEmit = '';
    pushLyric();
  } catch {
    if (token !== lyricToken) return;
    lines = [];
    pushLyric();
  }
}

export async function bindTrayBridge() {
  setInterval(() => {
    void loadLyricsForCurrent();
    pushLyric();
  }, 400);

  await listen<boolean>('main-window-visible', (e) => {
    mainVisible = !!e.payload;
    lastEmit = '';
    pushLyric();
  });

  await listen<string>('player-cmd', (e) => {
    switch (e.payload) {
      case 'toggle':
        togglePlay();
        break;
      case 'prev':
        prev();
        break;
      case 'next':
        next();
        break;
    }
  });
}
