// 与 Rust 端 model.rs / settings.rs 保持一致
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
  /** tlyric 对齐翻译，可能为空 */
  translation?: string | null;
}

export interface LyricPayload {
  lines: LyricLine[];
  raw: string;
  translated: string | null;
}

export type Quality = 'standard' | 'higher' | 'exhigh' | 'lossless' | 'hires';
export type AccentId = 'red' | 'terracotta';
export type ThemeAppearance = 'system' | 'light' | 'dark';

export interface Settings {
  quality: Quality;
  localFolders: string[];
  volume: number;
  shuffle: boolean;
  repeat: 'off' | 'all' | 'one';
  cacheEnabled: boolean;
  audioCacheMaxMb: number;
  /** 强调色：网易云红 / 陶土橙 */
  accent: AccentId;
  /** 主题：跟随系统 / 浅色 / 深色 */
  themeAppearance: ThemeAppearance;
}

/** 上次播放会话（playback.json） */
export interface PlaybackSession {
  queue: Track[];
  order: number[];
  pos: number;
  currentTime: number;
  wasPlaying: boolean;
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
