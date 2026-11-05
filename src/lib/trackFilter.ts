import type { LocalTrack, Track } from './types';

/** 小写 + 全角转半角，与后端 normalize 对齐 */
export function normalizeSearch(s: string): string {
  return s
    .replace(/[\uFF01-\uFF5E]/g, (c) => String.fromCharCode(c.charCodeAt(0) - 0xfee0))
    .replace(/\u3000/g, ' ')
    .toLowerCase();
}

function haystack(t: Track): string {
  const key = (t as LocalTrack).searchKey;
  if (key) return key;
  return normalizeSearch([t.title, ...t.artists, t.album].join(' '));
}

/** 歌单内筛选：多关键词 AND；有 searchKey（拼音）时用，否则标题/艺人/专辑。 */
export function filterTracks<T extends Track>(tracks: T[], query: string): T[] {
  const terms = normalizeSearch(query).split(/\s+/).filter(Boolean);
  if (!terms.length) return tracks;
  return tracks.filter((t) => {
    const h = haystack(t);
    return terms.every((q) => h.includes(q));
  });
}
