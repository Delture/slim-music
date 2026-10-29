// 本机播放列表缓存：与 player.queue（当前播放队列）完全独立
import { reactive } from 'vue';
import { api } from './api';
import type { LocalPlaylist, Track } from './types';

export const localPl = reactive({
  lists: [] as LocalPlaylist[],
  loaded: false,
});

export async function refreshLocalPlaylists() {
  localPl.lists = await api.localPlaylistsList();
  localPl.loaded = true;
}

export async function createLocalPlaylist(name: string) {
  const pl = await api.localPlaylistCreate(name);
  await refreshLocalPlaylists();
  return pl;
}

export async function renameLocalPlaylist(id: string, name: string) {
  await api.localPlaylistRename(id, name);
  await refreshLocalPlaylists();
}

export async function deleteLocalPlaylist(id: string) {
  await api.localPlaylistDelete(id);
  await refreshLocalPlaylists();
}

export async function addTrackToLocal(id: string, track: Track) {
  await api.localPlaylistAddTrack(id, track);
  await refreshLocalPlaylists();
}

export async function removeTrackFromLocal(id: string, trackId: string, index?: number) {
  await api.localPlaylistRemoveTrack(id, trackId, index);
  await refreshLocalPlaylists();
}

export async function moveLocalTrack(id: string, from: number, to: number) {
  const pl = await api.localPlaylistMoveTrack(id, from, to);
  const i = localPl.lists.findIndex((x) => x.id === id);
  if (i >= 0) localPl.lists[i] = pl;
  else await refreshLocalPlaylists();
  return pl;
}
