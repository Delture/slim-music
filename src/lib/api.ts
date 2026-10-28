// 所有后端调用集中在这里，方便替换/扩展
import { invoke } from '@tauri-apps/api/core';
import type { Playlist, Profile, Settings, Track } from './types';

export const api = {
  loginQrKey: () => invoke<{ key: string; svg: string }>('login_qr_key'),
  loginQrCheck: (key: string) => invoke<number>('login_qr_check', { key }),
  logout: () => invoke<void>('logout'),
  profile: () => invoke<Profile | null>('get_profile'),
  daily: () => invoke<Track[]>('daily_recommend'),
  search: (keywords: string, offset = 0) => invoke<Track[]>('search', { keywords, offset }),
  myPlaylists: () => invoke<Playlist[]>('my_playlists'),
  playlistTracks: (id: number) => invoke<Track[]>('playlist_tracks', { id }),
  songUrl: (id: number) => invoke<string | null>('song_url', { id }),
  getSettings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),
  scanLocal: () => invoke<Track[]>('scan_local'),
};
