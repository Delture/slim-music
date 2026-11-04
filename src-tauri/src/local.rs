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
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// 可扫描的扩展名；能否在当前平台播放由前端 canPlayType 再过滤。
pub const EXTS: [&str; 12] = [
    "mp3", "flac", "m4a", "aac", "wav", "ogg", "opus", "aiff", "aif", "ape", "wma", "wv",
];

const INDEX_VERSION: u32 = 1;
const COVER_BIG: u32 = 512;
const COVER_SMALL: u32 = 96;
const FOLDER_COVERS: [&str; 5] = ["cover", "folder", "front", "album", "albumart"];

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
}

pub struct Library {
    index_path: PathBuf,
    cover_dir: PathBuf,
}

impl Library {
    pub fn new(data_dir: &Path, cache_dir: &Path) -> Self {
        Self {
            index_path: data_dir.join("local_index.json"),
            cover_dir: cache_dir.join("local_covers"),
        }
    }

    fn load(&self) -> Index {
        std::fs::read(&self.index_path)
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

    /// 只读缓存（启动时秒开）；不访问磁盘上的音频文件。
    pub fn cached(&self) -> Vec<LocalTrack> {
        finish(self.load().entries.into_values().map(|e| e.track).collect())
    }

    /// 增量扫描：仅解析新增/变化的文件，删除已不存在的条目。
    pub fn scan(&self, folders: &[String]) -> Vec<LocalTrack> {
        let old = self.load();
        let mut next = Index { version: INDEX_VERSION, entries: HashMap::new() };
        let mut seen = HashSet::new();
        for folder in folders {
            let walker = WalkDir::new(folder)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| e.depth() == 0 || !is_hidden(e.file_name()));
            for entry in walker.filter_map(Result::ok) {
                if !entry.file_type().is_file() || !has_audio_ext(entry.path()) {
                    continue;
                }
                let path = entry.path().to_string_lossy().to_string();
                if !seen.insert(path.clone()) {
                    continue;
                }
                let (mtime, size) = entry
                    .metadata()
                    .map(|m| {
                        let mt = m
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        (mt, m.len())
                    })
                    .unwrap_or((0, 0));
                let e = match old.entries.get(&path) {
                    Some(e) if e.mtime == mtime && e.size == size => e.clone(),
                    _ => Entry {
                        mtime,
                        size,
                        track: read_track(entry.path(), mtime),
                        cover_done: false,
                    },
                };
                next.entries.insert(path, e);
            }
        }
        self.fill_covers(&mut next);
        self.save(&next);
        finish(next.entries.into_values().map(|e| e.track).collect())
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

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_str().map(|s| s.starts_with('.')).unwrap_or(false)
}

fn has_audio_ext(p: &Path) -> bool {
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
        if !matches!(ext.as_str(), "jpg" | "jpeg" | "png") {
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
    s.split(['/', ';', '；', '、', '\0'])
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

fn tag_str(tag: &Tag, key: &ItemKey) -> Option<String> {
    tag.get_string(key)
        .map(|v| fix_mojibake(v.trim()))
        .filter(|v| !v.is_empty())
}

fn read_track(p: &Path, mtime: u64) -> Track {
    let path = p.to_string_lossy().to_string();
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut t = Track {
        id: format!("local:{path}"),
        source: Source::Local,
        title: stem,
        path: Some(path),
        added_ms: Some(mtime),
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
    t.artists = artists;
    t.album = tag_str(tag, &ItemKey::AlbumTitle).unwrap_or_default();
    t.album_artist = tag_str(tag, &ItemKey::AlbumArtist);
    t.genre = tag_str(tag, &ItemKey::Genre);
    t.track_no = tag.track();
    t.disc_no = tag.disk();
    t.year = tag.year().or_else(|| {
        tag_str(tag, &ItemKey::RecordingDate)
            .and_then(|d| d.get(..4).and_then(|y| y.parse().ok()))
    });
    t
}

pub fn enrich(tracks: Vec<Track>) -> Vec<LocalTrack> {
    finish(tracks)
}

/// 预计算搜索 / 排序键（拼音全拼 + 首字母），按标题排序。
fn finish(tracks: Vec<Track>) -> Vec<LocalTrack> {
    let mut out: Vec<LocalTrack> = tracks
        .into_iter()
        .map(|t| {
            let (py_title, ini_title) = pinyin_of(&t.title);
            let mut hay = format!("{} {} {}", t.title, t.artists.join(" "), t.album);
            let (py_ar, ini_ar) = pinyin_of(&t.artists.join(" "));
            let (py_al, _) = pinyin_of(&t.album);
            hay.push(' ');
            hay.push_str(&[py_title.as_str(), &ini_title, &py_ar, &ini_ar, &py_al].join(" "));
            LocalTrack {
                search_key: normalize(&hay),
                sort_key: normalize(&py_title),
                track: t,
            }
        })
        .collect();
    out.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
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

/// 本地歌词：优先同名 .lrc，否则读内嵌歌词；都没有返回空 payload（不报错）。
pub fn load_local_lyric(audio_path: &str) -> Result<LyricPayload, String> {
    let audio = Path::new(audio_path);
    if !audio.is_file() {
        return Err(format!("本地文件不存在: {audio_path}"));
    }
    let raw = match sibling_lrc_path(audio) {
        Some(lrc) => decode_text(&std::fs::read(&lrc).map_err(|e| e.to_string())?),
        None => embedded_lyric(audio).unwrap_or_default(),
    };
    Ok(lyric_from_raw(raw))
}

/// 由原始 LRC 文本构造歌词；同一时间戳的第二行视为翻译（双语 LRC）。
pub fn lyric_from_raw(raw: String) -> LyricPayload {
    let raw = raw.trim_start_matches('\u{feff}').to_string();
    let lines = merge_bilingual(ncm::parse_lrc(&raw));
    LyricPayload { lines, raw, translated: None }
}

fn merge_bilingual(lines: Vec<LyricLine>) -> Vec<LyricLine> {
    let mut out: Vec<LyricLine> = Vec::with_capacity(lines.len());
    for l in lines {
        if let Some(prev) = out.last_mut() {
            if prev.time_ms == l.time_ms && prev.translation.is_none() && !l.text.is_empty() {
                if prev.text.is_empty() {
                    prev.text = l.text;
                } else {
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
    fn artists() {
        assert_eq!(split_artists("Simon & Garfunkel"), vec!["Simon & Garfunkel"]);
        assert_eq!(split_artists("A / B、C feat. D"), vec!["A", "B", "C", "D"]);
    }

    #[test]
    fn py() {
        assert_eq!(pinyin_of("晴天").1, "qt");
        assert_eq!(normalize("ＡＢＣ"), "abc");
    }
}
