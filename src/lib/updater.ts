// 应用内更新：桌面走 tauri-plugin-updater；Android 读 GitHub Releases 下载 APK 并调起安装器。
import { reactive } from 'vue';
import { getVersion } from '@tauri-apps/api/app';
import { isAndroid, nativeCall } from './native';

const REPO = 'Delture/slim-music';

export const updater = reactive({
  current: '',
  latest: '',
  available: false,
  checking: false,
  prompt: false,
  downloading: false,
  progress: 0, // 0..1，未知总长时为 -1
  error: '',
});

let desktopUpdate: import('@tauri-apps/plugin-updater').Update | null = null;
let apkUrl = '';

function cmp(a: string, b: string) {
  const p = (v: string) => v.replace(/^v/, '').split(/[.+-]/).slice(0, 3).map((x) => parseInt(x, 10) || 0);
  const x = p(a), y = p(b);
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
}

export async function checkUpdate(silent = false): Promise<boolean> {
  if (updater.checking || updater.downloading) return updater.available;
  updater.checking = true;
  updater.error = '';
  try {
    updater.current ||= await getVersion();
    if (isAndroid) {
      const r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
        headers: { Accept: 'application/vnd.github+json' },
      });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const j = await r.json();
      const asset = (j.assets ?? []).find((a: { name: string }) => /_android_arm64\.apk$/.test(a.name));
      updater.latest = String(j.tag_name ?? '').replace(/^v/, '');
      apkUrl = asset?.browser_download_url ?? '';
      updater.available = !!apkUrl && cmp(updater.latest, updater.current) > 0;
    } else {
      const { check } = await import('@tauri-apps/plugin-updater');
      desktopUpdate = await check();
      updater.available = !!desktopUpdate;
      updater.latest = desktopUpdate?.version ?? updater.current;
    }
    if (updater.available) updater.prompt = true;
  } catch (e) {
    if (!silent) updater.error = String(e);
  } finally {
    updater.checking = false;
  }
  return updater.available;
}

export function dismissUpdate() {
  updater.prompt = false;
}

export async function installUpdate() {
  if (!updater.available || updater.downloading) return;
  updater.downloading = true;
  updater.progress = 0;
  updater.error = '';
  try {
    if (isAndroid) {
      await nativeCall('downloadApk', { url: apkUrl });
      for (;;) {
        await new Promise((r) => setTimeout(r, 300));
        const s = await nativeCall<{ state: string; received: number; total: number; error: string }>('downloadStatus');
        updater.progress = s.total > 0 ? s.received / s.total : -1;
        if (s.state === 'error') throw new Error(s.error);
        if (s.state === 'done') break;
      }
      updater.progress = 1;
      await nativeCall('installApk');
    } else if (desktopUpdate) {
      let total = 0, got = 0;
      await desktopUpdate.downloadAndInstall((ev) => {
        if (ev.event === 'Started') total = ev.data.contentLength ?? 0;
        else if (ev.event === 'Progress') {
          got += ev.data.chunkLength;
          updater.progress = total > 0 ? got / total : -1;
        } else if (ev.event === 'Finished') updater.progress = 1;
      });
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    }
  } catch (e) {
    updater.error = String(e);
  } finally {
    updater.downloading = false;
  }
}
