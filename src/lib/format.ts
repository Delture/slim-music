export function fmtTime(sec: number): string {
  if (!isFinite(sec) || sec < 0) sec = 0;
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${s.toString().padStart(2, '0')}`;
}

/**
 * 网易云封面/头像：每日推荐等接口常给 http://，Tauri 正式包 webview 是 https 源会拦混合内容。
 * 已缓存的 Track.cover 也可能仍是 http，渲染前统一升 https。
 */
export function coverSrc(url: string | null | undefined, size?: string): string {
  if (!url) return '';
  let u = url.trim();
  if (u.startsWith('//')) u = `https:${u}`;
  else if (/^http:\/\//i.test(u)) u = `https://${u.slice(7)}`;
  if (size) {
    const base = u.split('?')[0];
    u = `${base}?param=${size}`;
  }
  return u;
}
