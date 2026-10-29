import { invoke } from '@tauri-apps/api/core';
import type { LocalPlaylist, LyricPayload, Playlist, Profile, Settings, Track } from './types';

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

  localPlaylistsList: () => invoke<LocalPlaylist[]>('local_playlists_list'),
  localPlaylistCreate: (name: string) => invoke<LocalPlaylist>('local_playlist_create', { name }),
  localPlaylistRename: (id: string, name: string) =>
    invoke<LocalPlaylist>('local_playlist_rename', { id, name }),
  localPlaylistDelete: (id: string) => invoke<void>('local_playlist_delete', { id }),
  localPlaylistGet: (id: string) => invoke<LocalPlaylist>('local_playlist_get', { id }),
  localPlaylistAddTrack: (id: string, track: Track) =>
    invoke<LocalPlaylist>('local_playlist_add_track', { id, track }),
  localPlaylistRemoveTrack: (id: string, trackId: string, index?: number | null) =>
    invoke<LocalPlaylist>('local_playlist_remove_track', { id, trackId, index: index ?? null }),
  localPlaylistMoveTrack: (id: string, from: number, to: number) =>
    invoke<LocalPlaylist>('local_playlist_move_track', { id, from, to }),
  localPlaylistSetTracks: (id: string, tracks: Track[]) =>
    invoke<LocalPlaylist>('local_playlist_set_tracks', { id, tracks }),
};
