//! 本地音乐扫描：遍历文件夹，用 lofty 读取标签。
use crate::model::{Source, Track};
use lofty::prelude::*;
use std::path::Path;
use walkdir::WalkDir;

const EXTS: [&str; 9] = ["mp3", "flac", "m4a", "aac", "ogg", "opus", "wav", "aiff", "ape"];

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
                t.artists = v.split(['/', ';', '&']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
            if let Some(v) = tag.album() {
                t.album = v.to_string();
            }
        }
    }
    t
}
