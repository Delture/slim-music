// 托盘 / 菜单栏标题：显示当前歌词行，没有歌词则显示歌名
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
  let text = lines[0]?.text ?? '';
  for (const l of lines) {
    if (l.timeMs <= ms) text = l.text;
    else break;
  }
  return text;
}

export function pushTray() {
  const t = current();
  const lyric = activeLine(player.currentTime * 1000);
  const text = lyric || (t ? `${t.title} - ${t.artists.join('/')}` : 'SlimMusic');
  if (text === lastEmit) return;
  lastEmit = text;
  void api.setTrayTitle(text).catch(() => {});
}

/** 在 App 挂载时调用一次 */
export async function bindTrayBridge() {
  // 时间更新节流推送
  let tick = 0;
  const onTime = () => {
    if (++tick % 4 !== 0) return; // ~1s if 4Hz timeupdate bursts
    pushTray();
  };
  // player.currentTime 由 audio timeupdate 写入；用 setInterval 更稳
  setInterval(() => {
    const t = current();
    const title = t?.id ?? '';
    if (title !== lastTitle) {
      lastTitle = title;
      lastEmit = '';
      pushTray();
    } else {
      pushTray();
    }
  }, 800);

  void onTime;

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
