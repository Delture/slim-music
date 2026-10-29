import { listen } from '@tauri-apps/api/event';

const el = document.getElementById('line')!;

void listen<string>('menubar-lyric-text', (e) => {
  el.textContent = (e.payload ?? '').trim();
});
