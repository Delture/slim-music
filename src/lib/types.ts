// 与 Rust 端 model.rs 保持一致
export type Source = 'local' | 'netease';

export interface Track {
  id: string;
  source: Source;
  title: string;
  artists: string[];
  album: string;
  durationMs: number;
  cover: string | null;
  path: string | null;
  ncmId: number | null;
}

export interface Playlist {
  id: number;
  name: string;
  cover: string | null;
  trackCount: number;
  isLiked: boolean;
  owned: boolean;
  subscribed: boolean;
  creatorId: number;
}

export interface Profile {
  userId: number;
  nickname: string;
  avatar: string | null;
}

export interface LyricLine {
  timeMs: number;
  text: string;
}

export interface LyricPayload {
  lines: LyricLine[];
  raw: string;
  translated: string | null;
}

export type Quality = 'standard' | 'higher' | 'exhigh' | 'lossless' | 'hires';

export interface Settings {
  quality: Quality;
  localFolders: string[];
  volume: number;
  cacheEnabled: boolean;
  audioCacheMaxMb: number;
}

export interface CacheHit<T> {
  data: T;
  fromCache: boolean;
  ageSecs: number;
}

export interface PlaySrc {
  kind: 'file' | 'url' | string;
  value: string;
  fromCache: boolean;
}

export interface CacheStats {
  totalBytes: number;
  metaBytes: number;
  audioBytes: number;
  audioFiles: number;
}

export const QUALITY_LABELS: Record<Quality, string> = {
  standard: '标准 (128k)',
  higher: '较高 (192k)',
  exhigh: '极高 (320k)',
  lossless: '无损 (FLAC)',
  hires: 'Hi-Res',
};

/** 本机播放列表（非网易云歌单；与当前播放队列无关） */
export interface LocalPlaylist {
  id: string;
  name: string;
  tracks: Track[];
  updatedAt: number;
}
