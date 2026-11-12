//! 本地音乐：增量扫描（持久索引）、标签解析（lofty）、封面缓存、歌词（.lrc / 内嵌）。
//!
//! 资源策略：
//! - 索引按 (mtime, size) 判断文件是否变化，未变化的文件不再解析标签；
//! - 扫描标签时不读封面（read_cover_art=false）；封面按专辑去重，仅缺失时读一次并缩放缓存；
//! - 不跟随符号链接，避免目录环。
use crate::model::{LyricLine, LyricPayload, Source, Track};
use crate::ncm;
use lofty::config::ParseOptions;
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{ItemKey, Tag};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// 可扫描的扩展名；能否在当前平台播放由前端 canPlayType 再过滤。
pub const EXTS: [&str; 13] = [
    "mp3", "flac", "m4a", "aac", "wav", "ogg", "oga", "opus", "aiff", "aif", "ape", "wma", "wv",
];

const INDEX_VERSION: u32 = 1;
const COVER_BIG: u32 = 512;
const COVER_SMALL: u32 = 96;
const FOLDER_COVERS: [&str; 8] = [
    "cover", "folder", "front", "album", "albumart", "artwork", "coverart", "frontcover",
];

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    mtime: u64,
    size: u64,
    track: Track,
    /// 已尝试过封面（有或无），避免每次扫描重试
    #[serde(default)]
    cover_done: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct Index {
    version: u32,
    entries: HashMap<String, Entry>,
}

/// 返回给前端的本地歌曲：Track + 预计算的搜索 / 排序键（不进播放列表存档）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTrack {
    #[serde(flatten)]
    pub track: Track,
    /// 搜索键：小写原文 + 全拼 + 首字母
    pub search_key: String,
    /// 标题排序键（拼音）
    pub sort_key: String,
    /// 与 artists 一一对应的分组键：繁→简 + 小写，合并「坂本龍一 / 坂本龙一」
    pub artist_keys: Vec<String>,
}

/// 文件夹监听增量更新的结果（不含整库曲目，前端再读 cached）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub library_changed: bool,
    /// 受影响的本地音频路径，前端据此作废歌词内存缓存
    pub lyrics_changed: Vec<String>,
}

pub struct Library {
    index_path: PathBuf,
    cover_dir: PathBuf,
    index: Mutex<Index>,
}

impl Library {
    pub fn new(data_dir: &Path, cache_dir: &Path) -> Self {
        let index_path = data_dir.join("local_index.json");
        let index = Self::read_index(&index_path);
        Self {
            index_path,
            cover_dir: cache_dir.join("local_covers"),
            index: Mutex::new(index),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Index> {
        self.index.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn read_index(path: &Path) -> Index {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Index>(&b).ok())
            .filter(|i| i.version == INDEX_VERSION)
            .unwrap_or(Index { version: INDEX_VERSION, entries: HashMap::new() })
    }

    fn save(&self, idx: &Index) {
        if let Ok(b) = serde_json::to_vec(idx) {
            let tmp = self.index_path.with_extension("json.tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, &self.index_path);
            }
        }
    }

    fn tracks_of(idx: &Index) -> Vec<LocalTrack> {
        finish(idx.entries.values().map(|e| e.track.clone()).collect(), true)
    }

    /// 只读缓存（启动时秒开）；不访问磁盘上的音频文件。
    pub fn cached(&self) -> Vec<LocalTrack> {
        Self::tracks_of(&self.lock())
    }

    /// 增量扫描：仅解析新增/变化的文件，删除已不存在的条目。
    pub fn scan(&self, folders: &[String]) -> Vec<LocalTrack> {
        let mut idx = self.lock();
        self.scan_into(&mut idx, folders);
        Self::tracks_of(&idx)
    }

    fn scan_into(&self, idx: &mut Index, folders: &[String]) {
        let old = std::mem::take(&mut idx.entries);
        idx.version = INDEX_VERSION;
        let mut seen = HashSet::new();
        for path in walk_audio(folders) {
            if !seen.insert(path.clone()) {
                continue;
            }
            let p = Path::new(&path);
            let (mtime, size) = file_stat(p);
            let e = match old.get(&path) {
                Some(e) if e.mtime == mtime && e.size == size => e.clone(),
                _ => Entry {
                    mtime,
                    size,
                    track: read_track(p, mtime),
                    cover_done: false,
                },
            };
            idx.entries.insert(path, e);
        }
        self.fill_covers(idx);
        self.save(idx);
    }

    /// 根据一批文件系统事件做增量更新，避免整库重扫。
    /// 事件过多（例如一次拷入大量文件）时退回增量全扫（仍按 mtime 跳过未变文件）。
    pub fn apply_paths(&self, folders: &[String], paths: &[PathBuf]) -> ApplyResult {
        let mut idx = self.lock();
        let mut result = ApplyResult::default();
        if folders.is_empty() {
            if !idx.entries.is_empty() {
                idx.entries.clear();
                self.save(&idx);
                result.library_changed = true;
            }
            return result;
        }
        let related: Vec<PathBuf> = paths
            .iter()
            .filter(|p| is_under_library(p, folders) || index_touches(&idx, p))
            .cloned()
            .collect();
        if related.is_empty() {
            return result;
        }
        const FULL_SCAN: usize = 80;
        if related.len() >= FULL_SCAN {
            self.scan_into(&mut idx, folders);
            result.library_changed = true;
            for p in &related {
                if is_lrc_path(p) {
                    result.lyrics_changed.extend(audios_for_lrc(&idx, p));
                }
            }
            dedup_paths(&mut result.lyrics_changed);
            return result;
        }
        for p in &related {
            self.apply_one(&mut idx, folders, p, &mut result);
        }
        dedup_paths(&mut result.lyrics_changed);
        if result.library_changed {
            self.fill_covers(&mut idx);
            self.save(&idx);
        }
        result
    }

    fn apply_one(&self, idx: &mut Index, folders: &[String], p: &Path, out: &mut ApplyResult) {
        if has_audio_ext(p) {
            if p.is_file() {
                if upsert_file(idx, p) {
                    out.library_changed = true;
                }
            } else if idx.entries.remove(&path_key(p)).is_some() {
                out.library_changed = true;
            }
            return;
        }
        if is_lrc_path(p) {
            out.lyrics_changed.extend(audios_for_lrc(idx, p));
            // 可能与尚未入索引的新音频成对出现：再按扩展名探测
            return;
        }
        if is_folder_cover_path(p) {
            if let Some(dir) = p.parent() {
                if invalidate_dir_covers(idx, dir, &self.cover_dir) {
                    out.library_changed = true;
                }
            }
            return;
        }
        if p.is_dir() {
            // 根目录的无关触摸不要整库重扫；新建的子目录要收入里面的音频
            if folders.iter().any(|f| Path::new(f) == p) {
                return;
            }
            if is_under_library(p, folders) {
                let mut changed = false;
                for f in walk_audio(&[p.to_string_lossy().to_string()]) {
                    if upsert_file(idx, Path::new(&f)) {
                        changed = true;
                    }
                }
                if changed {
                    out.library_changed = true;
                }
            }
            return;
        }
        // 已删除：目录或未知路径
        if !p.exists() {
            if is_lrc_path(p) {
                out.lyrics_changed.extend(audios_for_lrc(idx, p));
            }
            if remove_missing(idx, p) {
                out.library_changed = true;
            }
        }
    }

    /// 封面：按专辑键去重；缺失时读一次内嵌图片或同目录 cover.jpg，缩放成两种尺寸。
    fn fill_covers(&self, idx: &mut Index) {
        let _ = std::fs::create_dir_all(&self.cover_dir);
        // 专辑键 -> 结果（Some(path) 有图 / None 无图）
        let mut done: HashMap<String, Option<String>> = HashMap::new();
        let mut paths: Vec<String> = idx.entries.keys().cloned().collect();
        paths.sort();
        for path in paths {
            let e = idx.entries.get(&path).unwrap();
            let key = cover_key(&e.track, &path);
            let big = self.cover_dir.join(format!("{key}.jpg"));
            if e.cover_done && (e.track.cover.is_none() || big.is_file()) {
                if let Some(c) = &e.track.cover {
                    done.entry(key).or_insert_with(|| Some(c.clone()));
                }
                continue;
            }
            let res = if let Some(r) = done.get(&key) {
                r.clone()
            } else if big.is_file() {
                Some(big.to_string_lossy().to_string())
            } else {
                let r = load_cover_bytes(Path::new(&path))
                    .and_then(|b| write_cover(&b, &self.cover_dir, &key));
                r.map(|p| p.to_string_lossy().to_string())
            };
            // 只记录“有图”，无图的专辑里其他文件仍可能各自带图
            if res.is_some() {
                done.insert(key, res.clone());
            }
            let e = idx.entries.get_mut(&path).unwrap();
            e.track.cover = res;
            e.cover_done = true;
        }
    }
}

fn path_key(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

fn file_stat(p: &Path) -> (u64, u64) {
    std::fs::metadata(p)
        .map(|m| {
            let mt = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            (mt, m.len())
        })
        .unwrap_or((0, 0))
}

fn walk_audio(folders: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for folder in folders {
        let walker = WalkDir::new(folder)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !is_hidden(e.file_name()));
        for entry in walker.filter_map(Result::ok) {
            if entry.file_type().is_file() && has_audio_ext(entry.path()) {
                out.push(entry.path().to_string_lossy().to_string());
            }
        }
    }
    out
}

fn upsert_file(idx: &mut Index, p: &Path) -> bool {
    let key = path_key(p);
    let (mtime, size) = file_stat(p);
    if let Some(e) = idx.entries.get(&key) {
        if e.mtime == mtime && e.size == size {
            return false;
        }
    }
    idx.entries.insert(
        key,
        Entry {
            mtime,
            size,
            track: read_track(p, mtime),
            cover_done: false,
        },
    );
    true
}

fn is_under_library(p: &Path, folders: &[String]) -> bool {
    folders.iter().any(|f| p.starts_with(f))
}

fn index_touches(idx: &Index, p: &Path) -> bool {
    let k = path_key(p);
    if idx.entries.contains_key(&k) {
        return true;
    }
    let slash = format!("{k}/");
    let back = format!("{k}\\");
    idx.entries.keys().any(|e| e.starts_with(&slash) || e.starts_with(&back))
}

fn remove_missing(idx: &mut Index, p: &Path) -> bool {
    let k = path_key(p);
    let changed = idx.entries.remove(&k).is_some();
    let slash = format!("{k}/");
    let back = format!("{k}\\");
    let before = idx.entries.len();
    idx.entries
        .retain(|e, _| !(e.starts_with(&slash) || e.starts_with(&back)));
    changed || idx.entries.len() != before
}

fn invalidate_dir_covers(idx: &mut Index, dir: &Path, cover_dir: &Path) -> bool {
    let mut changed = false;
    let mut keys = HashSet::new();
    for (path, e) in idx.entries.iter_mut() {
        if Path::new(path).parent() == Some(dir) {
            if let Some(c) = e.track.cover.take() {
                if let Some(name) = Path::new(&c).file_stem() {
                    keys.insert(name.to_string_lossy().to_string());
                }
            }
            e.cover_done = false;
            changed = true;
        }
    }
    for key in keys {
        let _ = std::fs::remove_file(cover_dir.join(format!("{key}.jpg")));
        let _ = std::fs::remove_file(cover_dir.join(format!("{key}_s.jpg")));
    }
    changed
}

fn audios_for_lrc(idx: &Index, lrc: &Path) -> Vec<String> {
    let Some(stem) = audio_stem_from_lrc(lrc) else {
        return Vec::new();
    };
    let parent = lrc.parent();
    let stem_l = stem.to_ascii_lowercase();
    let mut out = Vec::new();
    for (k, e) in &idx.entries {
        let p = Path::new(k);
        if p.parent() != parent {
            continue;
        }
        let s = p
            .file_stem()
            .map(|x| x.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if s == stem_l {
            out.push(e.track.path.clone().unwrap_or_else(|| k.clone()));
        }
    }
    // 索引里还没有对应音频时，按常见扩展名探测
    if out.is_empty() {
        if let Some(dir) = parent {
            for ext in EXTS {
                for name in [format!("{stem}.{ext}"), format!("{stem}.{}", ext.to_uppercase())] {
                    let cand = dir.join(&name);
                    if cand.is_file() {
                        out.push(cand.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    out
}

fn dedup_paths(v: &mut Vec<String>) {
    v.sort();
    v.dedup();
}

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_str().map(|s| s.starts_with('.')).unwrap_or(false)
}

pub fn has_audio_ext(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn cover_key(t: &Track, path: &str) -> String {
    let who = t
        .album_artist
        .clone()
        .or_else(|| t.artists.first().cloned())
        .unwrap_or_default();
    let dir = Path::new(path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    // 无专辑名时按文件夹归组
    let k = if t.album.is_empty() {
        format!("dir\u{1}{dir}")
    } else {
        format!("{}\u{1}{}", who.to_lowercase(), t.album.to_lowercase())
    };
    format!("{:016x}", fnv(&k))
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn load_cover_bytes(p: &Path) -> Option<Vec<u8>> {
    let opts = ParseOptions::new().read_properties(false).read_cover_art(true);
    if let Ok(tagged) = Probe::open(p).and_then(|pr| pr.options(opts).read()) {
        for tag in tagged.tags() {
            let pics = tag.pictures();
            let pick = pics
                .iter()
                .find(|x| x.pic_type() == lofty::picture::PictureType::CoverFront)
                .or_else(|| pics.first());
            if let Some(pic) = pick {
                return Some(pic.data().to_vec());
            }
        }
    }
    let dir = p.parent()?;
    let rd = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(usize, PathBuf)> = None;
    for e in rd.flatten() {
        let fp = e.path();
        let ext = fp.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp") {
            continue;
        }
        let stem = fp.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if let Some(rank) = FOLDER_COVERS.iter().position(|n| *n == stem) {
            if best.as_ref().map(|(r, _)| rank < *r).unwrap_or(true) {
                best = Some((rank, fp));
            }
        }
    }
    best.and_then(|(_, fp)| std::fs::read(fp).ok())
}

fn write_cover(bytes: &[u8], dir: &Path, key: &str) -> Option<PathBuf> {
    let img = image::load_from_memory(bytes).ok()?;
    let big = dir.join(format!("{key}.jpg"));
    let small = dir.join(format!("{key}_s.jpg"));
    let save = |im: &image::DynamicImage, side: u32, out: &Path| -> Option<()> {
        let im = if im.width() > side || im.height() > side {
            im.resize_to_fill(side, side, image::imageops::FilterType::Triangle)
        } else {
            im.clone()
        };
        let rgb = im.to_rgb8();
        let mut f = std::io::BufWriter::new(std::fs::File::create(out).ok()?);
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 85);
        enc.encode_image(&rgb).ok()
    };
    save(&img, COVER_SMALL, &small)?;
    save(&img, COVER_BIG, &big)?;
    Some(big)
}

/// ID3v2.3/v1 常把 GBK 等本地编码当 Latin-1 写入：字符全部 ≤ U+00FF 且含高位时尝试重新探测。
pub fn fix_mojibake(s: &str) -> String {
    if !s.chars().any(|c| (c as u32) >= 0x80) || s.chars().any(|c| (c as u32) > 0xFF) {
        return s.to_string();
    }
    let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    det.feed(&bytes, true);
    let enc = det.guess(Some(b"cn"), chardetng::Utf8Detection::Allow);
    if enc == encoding_rs::WINDOWS_1252 {
        return s.to_string();
    }
    let (out, _, had_err) = enc.decode(&bytes);
    if had_err {
        s.to_string()
    } else {
        out.into_owned()
    }
}

/// 文本解码：UTF-8（含 BOM）/ UTF-16 BOM，否则探测 GBK / Big5 / Shift-JIS 等。
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some((enc, bom)) = encoding_rs::Encoding::for_bom(bytes) {
        return enc.decode_without_bom_handling(&bytes[bom..]).0.into_owned();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    det.feed(bytes, true);
    det.guess(Some(b"cn"), chardetng::Utf8Detection::Allow).decode_without_bom_handling(bytes).0.into_owned()
}

fn split_artists(v: &str) -> Vec<String> {
    let mut s = v.to_string();
    for sep in [" feat. ", " ft. ", " Feat. ", " FEAT. ", " featuring "] {
        s = s.replace(sep, "/");
    }
    s.split(['/', ';', '；', '、', ',', '，', '\0'])
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

fn tag_str(tag: &Tag, key: &ItemKey) -> Option<String> {
    tag.get_string(key)
        .map(|v| fix_mojibake(v.trim()))
        .filter(|v| !v.is_empty())
}

/// 无标签时从文件名猜测：「03 标题」「03 - 标题」「艺人 - 标题」。只填空字段。
pub fn guess_from_filename(stem: &str) -> (Option<u32>, Vec<String>, String) {
    let s = stem.replace('_', " ");
    let s = s.trim();
    let (track_no, rest) = {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if (1..=3).contains(&i) && i < s.len() {
            let sep = s[i..].chars().next();
            if matches!(sep, Some('.' | '-' | '–' | '—' | ' ' | '_')) {
                let n: u32 = s[..i].parse().unwrap_or(0);
                let rest = s[i..]
                    .trim_start_matches(|c: char| matches!(c, '.' | '-' | '–' | '—' | ' ' | '_'));
                if n > 0 && !rest.is_empty() {
                    (Some(n), rest)
                } else {
                    (None, s)
                }
            } else {
                (None, s)
            }
        } else {
            (None, s)
        }
    };
    for sep in [" - ", " – ", " — ", " － "] {
        if let Some((a, b)) = rest.split_once(sep) {
            let a = a.trim();
            let b = b.trim();
            if !a.is_empty() && !b.is_empty() {
                return (track_no, vec![a.to_string()], b.to_string());
            }
        }
    }
    (track_no, vec![], rest.trim().to_string())
}

fn read_track(p: &Path, mtime: u64) -> Track {
    let path = p.to_string_lossy().to_string();
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let (guess_no, guess_artists, guess_title) = guess_from_filename(&stem);
    let mut t = Track {
        id: format!("local:{path}"),
        source: Source::Local,
        title: if guess_title.is_empty() { stem } else { guess_title },
        artists: guess_artists,
        path: Some(path),
        added_ms: Some(mtime),
        track_no: guess_no,
        ..Default::default()
    };
    let opts = ParseOptions::new().read_cover_art(false);
    // 标签读取失败时退回到文件名
    let Ok(tagged) = Probe::open(p).and_then(|pr| pr.options(opts).read()) else {
        return t;
    };
    t.duration_ms = tagged.properties().duration().as_millis() as u64;
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return t;
    };
    if let Some(v) = tag_str(tag, &ItemKey::TrackTitle) {
        t.title = v;
    }
    let mut artists: Vec<String> = Vec::new();
    for v in tag.get_strings(&ItemKey::TrackArtist) {
        for a in split_artists(&fix_mojibake(v)) {
            if !artists.contains(&a) {
                artists.push(a);
            }
        }
    }
    if !artists.is_empty() {
        t.artists = artists;
    }
    t.album = tag_str(tag, &ItemKey::AlbumTitle).unwrap_or_default();
    t.album_artist = tag_str(tag, &ItemKey::AlbumArtist);
    t.genre = tag_str(tag, &ItemKey::Genre);
    if let Some(n) = tag.track() {
        t.track_no = Some(n);
    }
    t.disc_no = tag.disk();
    t.year = tag.year().or_else(|| {
        tag_str(tag, &ItemKey::RecordingDate)
            .and_then(|d| d.get(..4).and_then(|y| y.parse().ok()))
    });
    t
}

pub fn enrich(tracks: Vec<Track>) -> Vec<LocalTrack> {
    // 必须保留入参顺序：网易云「我喜欢的音乐」按收藏时间；本机歌单按用户排列。
    finish(tracks, false)
}

/// 预计算搜索 / 排序键（拼音全拼 + 首字母）。
/// `sort_by_title`：仅本地曲库扫描用；歌单 enrich 必须为 false。
fn finish(tracks: Vec<Track>, sort_by_title: bool) -> Vec<LocalTrack> {
    let mut out: Vec<LocalTrack> = tracks
        .into_iter()
        .map(|mut t| {
            // 统一拆分（Android MediaStore、旧索引都走这里）
            let mut artists: Vec<String> = Vec::new();
            for a in t.artists.iter().flat_map(|a| split_artists(a)) {
                if !artists.iter().any(|x| x.eq_ignore_ascii_case(&a)) {
                    artists.push(a);
                }
            }
            t.artists = artists;
            let artist_keys = t.artists.iter().map(|a| normalize(&fast2s::convert(a))).collect();
            let (py_title, ini_title) = pinyin_of(&t.title);
            let mut hay = format!("{} {} {}", t.title, t.artists.join(" "), t.album);
            let (py_ar, ini_ar) = pinyin_of(&t.artists.join(" "));
            let (py_al, _) = pinyin_of(&t.album);
            hay.push(' ');
            hay.push_str(&[py_title.as_str(), &ini_title, &py_ar, &ini_ar, &py_al].join(" "));
            let simp = fast2s::convert(&hay);
            if simp != hay {
                hay.push(' ');
                hay.push_str(&simp);
            }
            LocalTrack {
                artist_keys,
                search_key: normalize(&hay),
                sort_key: normalize(&py_title),
                track: t,
            }
        })
        .collect();
    if sort_by_title {
        out.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
    }
    out
}

/// 汉字转拼音（无声调），返回 (全拼无空格, 首字母)；非汉字原样保留。
pub fn pinyin_of(s: &str) -> (String, String) {
    use pinyin::ToPinyin;
    let mut full = String::new();
    let mut ini = String::new();
    for (c, p) in s.chars().zip(s.to_pinyin()) {
        match p {
            Some(p) => {
                full.push_str(p.plain());
                ini.push_str(p.first_letter());
            }
            None => {
                full.push(c);
                if c.is_alphanumeric() {
                    ini.push(c);
                }
            }
        }
    }
    (full, ini)
}

/// 小写 + 全角转半角
pub fn normalize(s: &str) -> String {
    s.chars()
        .map(|c| match c as u32 {
            0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            0x3000 => ' ',
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// 译文 sidecar 文件名介于主文件名与 `.lrc` 之间的中缀（大小写不敏感）。
/// 仅在同名主 `.lrc` 已存在时采用，避免把「歌名.zh.lrc」单独当成译文。
const TRANS_MIDS: &[&str] = &[
    ".trans",
    ".tlyric",
    ".translation",
    ".zh-hans",
    ".zh-hant",
    ".zh-cn",
    ".zh-tw",
    ".zh",
    ".cn",
    ".chs",
    ".cht",
    ".翻译",
    ".译",
    "_trans",
    "_zh",
];

pub fn is_lrc_path(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("lrc"))
        .unwrap_or(false)
}

pub fn is_folder_cover_path(p: &Path) -> bool {
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp") {
        return false;
    }
    let stem = p
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .replace(' ', "");
    FOLDER_COVERS.iter().any(|n| stem == *n)
}

/// 音频同目录、同主文件名的 .lrc（大小写不敏感）。
fn sibling_lrc_path(audio: &Path) -> Option<PathBuf> {
    let parent = audio.parent()?;
    let stem = audio.file_stem()?.to_string_lossy();
    for name in [format!("{stem}.lrc"), format!("{stem}.LRC")] {
        let p = parent.join(&name);
        if p.is_file() {
            return Some(p);
        }
    }
    let want = format!("{stem}.lrc").to_ascii_lowercase();
    if let Ok(rd) = std::fs::read_dir(parent) {
        for e in rd.flatten() {
            let name = e.file_name();
            let s = name.to_string_lossy();
            if s.to_ascii_lowercase() == want && e.path().is_file() {
                return Some(e.path());
            }
        }
    }
    None
}

fn mid_matches_trans(mid: &str) -> Option<usize> {
    let lower = mid.to_ascii_lowercase();
    TRANS_MIDS
        .iter()
        .position(|m| mid == *m || lower == m.to_ascii_lowercase())
}

/// 主 `.lrc` 旁的译文文件，例如 `歌名.trans.lrc`、`歌名.翻译.lrc`。
fn sibling_trans_lrc(audio: &Path, main_lrc: &Path) -> Option<PathBuf> {
    let parent = audio.parent()?;
    let stem = audio.file_stem()?.to_string_lossy();
    let stem_l = stem.to_ascii_lowercase();
    let main_name = main_lrc.file_name()?.to_string_lossy().to_ascii_lowercase();
    let rd = std::fs::read_dir(parent).ok()?;
    let mut found: Option<(usize, PathBuf)> = None;
    for e in rd.flatten() {
        let path = e.path();
        if !path.is_file() || !is_lrc_path(&path) {
            continue;
        }
        let name = e.file_name();
        let s = name.to_string_lossy();
        let lower = s.to_ascii_lowercase();
        if lower == main_name {
            continue;
        }
        if !lower.starts_with(&stem_l) || !lower.ends_with(".lrc") {
            continue;
        }
        let mid_end = lower.len() - 4;
        if mid_end < stem_l.len() {
            continue;
        }
        let mid = &s[stem.len()..s.len() - 4];
        // 用原始切片的长度，大小写用 lower 的对应段比较
        let mid_l = &lower[stem_l.len()..mid_end];
        if let Some(rank) = mid_matches_trans(mid).or_else(|| mid_matches_trans(mid_l)) {
            if found.as_ref().map(|(r, _)| rank < *r).unwrap_or(true) {
                found = Some((rank, path));
            }
        }
    }
    found.map(|(_, p)| p)
}

/// `.lrc`（含译文后缀）对应的音频主文件名。
pub fn audio_stem_from_lrc(lrc: &Path) -> Option<String> {
    let stem = lrc.file_stem()?.to_string_lossy();
    let lower = stem.to_ascii_lowercase();
    for mid in TRANS_MIDS {
        let mid_l = mid.to_ascii_lowercase();
        if lower.ends_with(&mid_l) && stem.len() > mid.len() {
            return Some(stem[..stem.len() - mid.len()].to_string());
        }
    }
    Some(stem.to_string())
}

fn file_mtime_ms(p: &Path) -> u64 {
    std::fs::metadata(p)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 歌词源（sidecar / 译文 sidecar / 音频内嵌）的最新 mtime，用于磁盘缓存失效。
pub fn lyric_source_mtime(audio_path: &str) -> u64 {
    let audio = Path::new(audio_path);
    if let Some(lrc) = sibling_lrc_path(audio) {
        let mut mt = file_mtime_ms(&lrc);
        if let Some(t) = sibling_trans_lrc(audio, &lrc) {
            mt = mt.max(file_mtime_ms(&t));
        }
        return mt;
    }
    if audio.is_file() {
        return file_mtime_ms(audio);
    }
    0
}

fn embedded_lyric(audio: &Path) -> Option<String> {
    let opts = ParseOptions::new().read_properties(false).read_cover_art(false);
    let tagged = Probe::open(audio).and_then(|pr| pr.options(opts).read()).ok()?;
    for tag in tagged.tags() {
        for key in [ItemKey::Lyrics, ItemKey::Unknown("UNSYNCEDLYRICS".into())] {
            if let Some(v) = tag.get_string(&key) {
                if !v.trim().is_empty() {
                    return Some(fix_mojibake(v));
                }
            }
        }
    }
    None
}

pub struct LocalLyric {
    pub payload: LyricPayload,
    pub source_mtime: u64,
}

/// 本地歌词：优先同名 .lrc（及译文 sidecar），否则读内嵌歌词；都没有返回空 payload。
pub fn load_local_lyric(audio_path: &str) -> Result<LocalLyric, String> {
    let audio = Path::new(audio_path);
    if !audio.is_file() {
        return Err(format!("本地文件不存在: {audio_path}"));
    }
    let (raw, trans, mtime) = match sibling_lrc_path(audio) {
        Some(lrc) => {
            let raw = decode_text(&std::fs::read(&lrc).map_err(|e| e.to_string())?);
            let trans_path = sibling_trans_lrc(audio, &lrc);
            let trans = trans_path
                .as_ref()
                .and_then(|p| std::fs::read(p).ok())
                .map(|b| decode_text(&b));
            let mut mt = file_mtime_ms(&lrc);
            if let Some(t) = &trans_path {
                mt = mt.max(file_mtime_ms(t));
            }
            (raw, trans, mt)
        }
        None => (
            embedded_lyric(audio).unwrap_or_default(),
            None,
            file_mtime_ms(audio),
        ),
    };
    Ok(LocalLyric {
        payload: lyric_from_raw_pair(raw, trans),
        source_mtime: mtime,
    })
}

/// Android 原生层返回的 .lrc 原始字节（base64），在此探测编码。
pub fn lyric_from_b64(b64: &str) -> LyricPayload {
    lyric_from_b64_pair(b64, None)
}

pub fn lyric_from_b64_pair(b64: &str, trans_b64: Option<&str>) -> LyricPayload {
    use base64::Engine;
    let eng = base64::engine::general_purpose::STANDARD;
    let raw = decode_text(&eng.decode(b64).unwrap_or_default());
    let trans = trans_b64
        .filter(|s| !s.is_empty())
        .map(|s| decode_text(&eng.decode(s).unwrap_or_default()));
    lyric_from_raw_pair(raw, trans)
}

/// 由原始 LRC 文本构造歌词；同一时间戳的第二行或行内「原文 / 译文」视为翻译。
pub fn lyric_from_raw(raw: String) -> LyricPayload {
    lyric_from_raw_pair(raw, None)
}

pub fn lyric_from_raw_pair(raw: String, trans: Option<String>) -> LyricPayload {
    let raw = raw.trim_start_matches('\u{feff}').to_string();
    let mut lines = merge_bilingual(ncm::parse_lrc(&raw));
    for l in &mut lines {
        if l.translation.is_none() {
            if let Some((a, b)) = split_inline_trans(&l.text) {
                l.text = a;
                l.translation = Some(b);
            }
        }
    }
    let translated = trans
        .map(|t| t.trim_start_matches('\u{feff}').to_string())
        .filter(|t| !t.trim().is_empty());
    if let Some(t) = &translated {
        lines = ncm::merge_translations(lines, t);
    }
    LyricPayload {
        lines,
        raw,
        translated,
    }
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        let u = c as u32;
        (0x4E00..=0x9FFF).contains(&u)
            || (0x3400..=0x4DBF).contains(&u)
            || (0x3040..=0x30FF).contains(&u)
            || (0xAC00..=0xD7AF).contains(&u)
    })
}

/// 常见中文双语写法：`Hello / 你好` 或全角 `／`；仅在一侧含汉字时拆分。
fn split_inline_trans(text: &str) -> Option<(String, String)> {
    for sep in ["／", " / "] {
        if let Some((a, b)) = text.split_once(sep) {
            let a = a.trim();
            let b = b.trim();
            if a.is_empty() || b.is_empty() {
                continue;
            }
            if has_cjk(a) != has_cjk(b) {
                return Some((a.to_string(), b.to_string()));
            }
        }
    }
    None
}

fn merge_bilingual(lines: Vec<LyricLine>) -> Vec<LyricLine> {
    let mut out: Vec<LyricLine> = Vec::with_capacity(lines.len());
    for l in lines {
        if let Some(prev) = out.last_mut() {
            if prev.time_ms == l.time_ms && prev.translation.is_none() && !l.text.is_empty() {
                if prev.text.is_empty() {
                    prev.text = l.text;
                } else if prev.text != l.text {
                    prev.translation = Some(l.text);
                }
                continue;
            }
        }
        out.push(l);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gbk_mojibake() {
        let (gbk, _, _) = encoding_rs::GBK.encode("周杰伦 晴天");
        let latin: String = gbk.iter().map(|&b| b as char).collect();
        assert_eq!(fix_mojibake(&latin), "周杰伦 晴天");
        assert_eq!(fix_mojibake("Beyoncé"), "Beyoncé");
    }

    #[test]
    fn gbk_lrc() {
        let (b, _, _) = encoding_rs::GBK.encode("[00:01.00]你好");
        assert_eq!(decode_text(&b), "[00:01.00]你好");
    }

    #[test]
    fn bilingual() {
        let p = lyric_from_raw("[00:01.00]Hello\n[00:01.00]你好\n[00:02.00]World".into());
        assert_eq!(p.lines.len(), 2);
        assert_eq!(p.lines[0].translation.as_deref(), Some("你好"));
    }

    #[test]
    fn enrich_preserves_order() {
        let tracks = vec![
            Track {
                title: "斑马".into(),
                ..Default::default()
            },
            Track {
                title: "苹果".into(),
                ..Default::default()
            },
            Track {
                title: "啊啊".into(),
                ..Default::default()
            },
        ];
        let r = enrich(tracks);
        assert_eq!(r[0].track.title, "斑马");
        assert_eq!(r[1].track.title, "苹果");
        assert_eq!(r[2].track.title, "啊啊");
        // 若按拼音排序会变成 啊啊、斑马、苹果
        assert!(r[0].sort_key > r[2].sort_key);
    }

    #[test]
    fn artists() {
        assert_eq!(split_artists("Simon & Garfunkel"), vec!["Simon & Garfunkel"]);
        assert_eq!(split_artists("A / B、C feat. D"), vec!["A", "B", "C", "D"]);
        assert_eq!(split_artists("蔡依林，周杰伦, X"), vec!["蔡依林", "周杰伦", "X"]);
        let t = Track { artists: vec!["坂本龍一、Jaques Morelenbaum".into()], ..Default::default() };
        let r = enrich(vec![t]);
        assert_eq!(r[0].track.artists.len(), 2);
        assert_eq!(r[0].artist_keys[0], "坂本龙一");
    }

    #[test]
    fn py() {
        assert_eq!(pinyin_of("晴天").1, "qt");
        assert_eq!(normalize("ＡＢＣ"), "abc");
    }

    #[test]
    fn filename_guess() {
        assert_eq!(
            guess_from_filename("03 - 晴天"),
            (Some(3), vec![], "晴天".into())
        );
        assert_eq!(
            guess_from_filename("周杰伦 - 晴天"),
            (None, vec!["周杰伦".into()], "晴天".into())
        );
        assert_eq!(
            guess_from_filename("02. Artist - Title"),
            (Some(2), vec!["Artist".into()], "Title".into())
        );
        assert_eq!(guess_from_filename("1989").2, "1989");
        assert_eq!(guess_from_filename("1989").0, None);
    }

    #[test]
    fn inline_and_sidecar_trans() {
        let p = lyric_from_raw("[00:01.00]Hello / 你好\n[00:02.00]World／世界".into());
        assert_eq!(p.lines.len(), 2);
        assert_eq!(p.lines[0].text, "Hello");
        assert_eq!(p.lines[0].translation.as_deref(), Some("你好"));
        assert_eq!(p.lines[1].text, "World");
        assert_eq!(p.lines[1].translation.as_deref(), Some("世界"));
        // 两侧都是拉丁文，不拆
        let q = lyric_from_raw("[00:01.00]AC / DC".into());
        assert_eq!(q.lines[0].text, "AC / DC");
        assert!(q.lines[0].translation.is_none());

        let pair = lyric_from_raw_pair(
            "[00:01.00]Hello\n[00:02.00]World".into(),
            Some("[00:01.00]你好\n[00:02.00]世界".into()),
        );
        assert_eq!(pair.lines[0].translation.as_deref(), Some("你好"));
        assert_eq!(pair.lines[1].translation.as_deref(), Some("世界"));
    }

    #[test]
    fn apply_paths_add_and_remove() {
        let stamp = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("slim-local-{stamp}"));
        let music = root.join("music");
        let data = root.join("data");
        let cache = root.join("cache");
        std::fs::create_dir_all(&music).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let lib = Library::new(&data, &cache);
        let folders = [music.to_string_lossy().to_string()];
        assert!(lib.cached().is_empty());

        let song = music.join("周杰伦 - 晴天.mp3");
        std::fs::write(&song, b"not a real mp3").unwrap();
        let r = lib.apply_paths(&folders, &[song.clone()]);
        assert!(r.library_changed);
        let tracks = lib.cached();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].track.title, "晴天");
        assert_eq!(tracks[0].track.artists, vec!["周杰伦"]);

        let lrc = music.join("周杰伦 - 晴天.lrc");
        std::fs::write(&lrc, "[00:01.00]晴天\n[00:01.00]Sunny Day\n").unwrap();
        let r = lib.apply_paths(&folders, &[lrc.clone()]);
        assert!(!r.library_changed);
        assert_eq!(r.lyrics_changed.len(), 1);
        let ly = load_local_lyric(&song.to_string_lossy()).unwrap();
        assert_eq!(ly.payload.lines[0].text, "晴天");
        assert_eq!(ly.payload.lines[0].translation.as_deref(), Some("Sunny Day"));

        std::fs::remove_file(&song).unwrap();
        let r = lib.apply_paths(&folders, &[song.clone()]);
        assert!(r.library_changed);
        assert!(lib.cached().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lrc_stem_strips_trans_mid() {
        assert_eq!(
            audio_stem_from_lrc(Path::new("/m/晴天.trans.lrc")).as_deref(),
            Some("晴天")
        );
        assert_eq!(
            audio_stem_from_lrc(Path::new("/m/晴天.lrc")).as_deref(),
            Some("晴天")
        );
        assert_eq!(
            audio_stem_from_lrc(Path::new("/m/晴天.翻译.lrc")).as_deref(),
            Some("晴天")
        );
    }
}
