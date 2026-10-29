// Android 原生桥（Media3 前台服务 / MediaStore 等）；桌面端不使用。
import { Channel, invoke } from '@tauri-apps/api/core';

export const isAndroid = typeof navigator !== 'undefined' && /Android/i.test(navigator.userAgent);
export const isMobile = isAndroid || (typeof navigator !== 'undefined' && /iPhone|iPad/i.test(navigator.userAgent));

export function nativeCall<T = unknown>(method: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>('native_call', { method, args: args ?? {} });
}

export type NativeEvent =
  | { type: 'state'; playing: boolean; playWhenReady: boolean; buffering: boolean; position: number; duration: number }
  | { type: 'ended' }
  | { type: 'error'; message: string }
  | { type: 'remote'; action: 'next' | 'prev' };

export async function nativeListen(cb: (e: NativeEvent) => void) {
  const ch = new Channel<NativeEvent>();
  ch.onmessage = cb;
  await invoke('native_listen', { onEvent: ch });
}

export interface NativeAudio {
  uri: string;
  title: string;
  artist: string;
  album: string;
  durationMs: number;
  path: string;
  albumArt: string;
}

export async function queryLocalAudio(): Promise<NativeAudio[]> {
  const { granted } = await nativeCall<{ granted: boolean }>('requestAudioPermission');
  if (!granted) throw new Error('未授予音频读取权限');
  const r = await nativeCall<{ tracks: NativeAudio[] }>('queryAudio');
  return r.tracks ?? [];
}

export async function openExternal(url: string): Promise<boolean> {
  const r = await nativeCall<{ ok: boolean }>('openUrl', { url });
  return !!r?.ok;
}

export async function saveImage(dataUrl: string, name: string) {
  await nativeCall('saveImage', { data: dataUrl, name });
}
