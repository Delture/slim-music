import { invoke } from '@tauri-apps/api/core';
import type { LyricPayload, Playlist, Profile, Settings, Track } from './types';

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
  likedIds: () => invoke<number[]>('liked_ids'),
  likeTrack: (id: number, like: boolean) => invoke<void>('like_track', { id, like }),
  playlistSubscribe: (id: number, subscribe: boolean) =>
    invoke<void>('playlist_subscribe', { id, subscribe }),
  lyric: (id: number) => invoke<LyricPayload>('get_lyric', { id }),
  setTrayTitle: (text: string) => invoke<void>('set_tray_title', { text }),
  showMainWindow: () => invoke<void>('show_main_window'),
  quitApp: () => invoke<void>('quit_app'),
};
