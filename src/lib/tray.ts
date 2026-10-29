// 托盘 / 菜单栏：图标旁只显示当前歌词行（短截断）；无歌词则清空标题
import { listen } from '@tauri-apps/api/event';
import { api } from './api';
import { current, next, player, prev, togglePlay } from './player';
import type { LyricLine } from './types';

let lines: LyricLine[] = [];
let lastTitle = '';
let lastEmit = '';

export function setTrayLyricLines(l: LyricLine[]) {
  lines = l;
  pushTray();
}

export function clearTrayLyric() {
  lines = [];
  pushTray();
}

function activeLine(ms: number): string {
  if (!lines.length) return '';
  let text = '';
  for (const l of lines) {
    if (l.timeMs <= ms) text = l.text;
    else break;
  }
  return text.trim();
}

export function pushTray() {
  // 只要歌词；没有就传空字符串让 Rust 清掉菜单栏标题
  const text = activeLine(player.currentTime * 1000);
  if (text === lastEmit) return;
  lastEmit = text;
  void api.setTrayTitle(text).catch(() => {});
}

export async function bindTrayBridge() {
  setInterval(() => {
    const t = current();
    const id = t?.id ?? '';
    if (id !== lastTitle) {
      lastTitle = id;
      lastEmit = '';
      // 切歌时先清空，等歌词加载后再显示
      void api.setTrayTitle('').catch(() => {});
    }
    pushTray();
  }, 500);

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
