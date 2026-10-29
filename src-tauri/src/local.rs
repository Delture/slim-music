//! 本地音乐扫描：遍历文件夹，用 lofty 读取标签；同名 .lrc 作为歌词。
use crate::model::{LyricPayload, Source, Track};
use crate::ncm;
use lofty::prelude::*;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 常见有损/无损扩展名。wma 可扫到；WKWebView 未必能解码播放。
const EXTS: [&str; 12] = [
    "mp3", "flac", "m4a", "aac", "wav", "ogg", "opus", "aiff", "aif", "ape", "wma", "wv",
];

pub fn scan(folders: &[String]) -> Vec<Track> {
    let mut out = Vec::new();
    for folder in folders {
        for entry in WalkDir::new(folder).follow_links(true).into_iter().filter_map(Result::ok) {
            let p = entry.path();
            let ok = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| EXTS.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false);
            if entry.file_type().is_file() && ok {
                out.push(read_track(p));
            }
        }
    }
    out.sort_by(|a, b| a.title.cmp(&b.title));
    out
}

fn read_track(p: &Path) -> Track {
    let path = p.to_string_lossy().to_string();
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut t = Track {
        id: format!("local:{path}"),
        source: Source::Local,
        title: stem,
        artists: vec![],
        album: String::new(),
        duration_ms: 0,
        cover: None,
        path: Some(path),
        ncm_id: None,
    };
    // 标签读取失败时退回到文件名
    if let Ok(tagged) = lofty::read_from_path(p) {
        t.duration_ms = tagged.properties().duration().as_millis() as u64;
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(v) = tag.title() {
                t.title = v.to_string();
            }
            if let Some(v) = tag.artist() {
                t.artists = v
                    .split(['/', ';', '&'])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            if let Some(v) = tag.album() {
                t.album = v.to_string();
            }
        }
    }
    t
}

/// 音频同目录、同主文件名的 .lrc / .LRC（不做封面旁路）。
fn sibling_lrc_path(audio: &Path) -> Option<PathBuf> {
    let parent = audio.parent()?;
    let stem = audio.file_stem()?.to_string_lossy();
    for name in [format!("{stem}.lrc"), format!("{stem}.LRC")] {
        let p = parent.join(&name);
        if p.is_file() {
            return Some(p);
        }
    }
    // 大小写不敏感再扫一遍同目录
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

/// 读取本地音频旁的 .lrc；没有则返回空 payload（不报错）。
pub fn load_sidecar_lrc(audio_path: &str) -> Result<LyricPayload, String> {
    let audio = Path::new(audio_path);
    if !audio.is_file() {
        return Err(format!("本地文件不存在: {audio_path}"));
    }
    let Some(lrc) = sibling_lrc_path(audio) else {
        return Ok(LyricPayload {
            lines: vec![],
            raw: String::new(),
            translated: None,
        });
    };
    let bytes = std::fs::read(&lrc).map_err(|e| e.to_string())?;
    // UTF-8 BOM
    let raw = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let lines = ncm::parse_lrc(&raw);
    Ok(LyricPayload {
        lines,
        raw,
        translated: None,
    })
}

/// 由原始 LRC 文本构造歌词（Android 由原生层读取同名 .lrc）。
pub fn lyric_from_raw(raw: String) -> LyricPayload {
    let raw = raw.trim_start_matches('\u{feff}').to_string();
    let lines = ncm::parse_lrc(&raw);
    LyricPayload {
        lines,
        raw,
        translated: None,
    }
}
