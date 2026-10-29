// 托盘：右侧图标 + 右键菜单控制播放；不显示歌词标题。
import { listen } from '@tauri-apps/api/event';
import { next, prev, togglePlay } from './player';
import type { LyricLine } from './types';

/** NowPlaying 仍可调用；菜单栏不再展示，保留空实现以免改动调用方。 */
export function setTrayLyricLines(_l: LyricLine[]) {}
export function clearTrayLyric() {}

export async function bindTrayBridge() {
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
