//! 设置与登录 cookie 持久化（应用数据目录下的 JSON 文件）。
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

pub const QUALITY_LEVELS: [&str; 5] = ["standard", "higher", "exhigh", "lossless", "hires"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 网易云音质：standard / higher / exhigh / lossless / hires
    pub quality: String,
    /// 本地音乐文件夹
    pub local_folders: Vec<String>,
    pub volume: f64,
    /// 随机播放
    #[serde(default)]
    pub shuffle: bool,
    /// off / all / one
    #[serde(default = "default_repeat")]
    pub repeat: String,
    /// 启用歌单/音频本地缓存
    pub cache_enabled: bool,
    /// 音频缓存上限（MB）
    pub audio_cache_max_mb: u64,
}

fn default_repeat() -> String {
    "off".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quality: "exhigh".into(),
            local_folders: vec![],
            volume: 0.8,
            shuffle: false,
            repeat: "off".into(),
            cache_enabled: true,
            audio_cache_max_mb: 2048,
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlaybackSession {
    pub queue: Vec<crate::model::Track>,
    /// queue 下标序列（含随机顺序）
    pub order: Vec<usize>,
    /// order 中的位置
    pub pos: i32,
    pub current_time: f64,
    pub was_playing: bool,
}

fn track_playable(t: &crate::model::Track) -> bool {
    match t.source {
        crate::model::Source::Local => t
            .path
            .as_ref()
            .map(|p| Path::new(p).is_file())
            .unwrap_or(false),
        crate::model::Source::Netease => t.ncm_id.is_some(),
    }
}

/// 去掉失效本地路径 / 缺 id 的网易云曲目，并重映射 order
pub fn sanitize_session(mut s: PlaybackSession) -> PlaybackSession {
    const MAX: usize = 500;
    if s.queue.len() > MAX {
        s.queue.truncate(MAX);
    }
    let mut map = vec![None; s.queue.len()];
    let mut new_q = Vec::with_capacity(s.queue.len());
    for (i, t) in s.queue.into_iter().enumerate() {
        if track_playable(&t) {
            map[i] = Some(new_q.len());
            new_q.push(t);
        }
    }
    let mut new_order = Vec::new();
    for &qi in &s.order {
        if let Some(Some(ni)) = map.get(qi) {
            new_order.push(*ni);
        }
    }
    // order 缺漏时补全
    if new_order.len() != new_q.len() {
        let mut seen = vec![false; new_q.len()];
        for &i in &new_order {
            if i < seen.len() {
                seen[i] = true;
            }
        }
        for i in 0..new_q.len() {
            if !seen[i] {
                new_order.push(i);
            }
        }
    }
    let pos = if new_order.is_empty() {
        -1
    } else {
        let p = s.pos;
        if p >= 0 && (p as usize) < new_order.len() {
            p
        } else {
            0
        }
    };
    PlaybackSession {
        queue: new_q,
        order: new_order,
        pos,
        current_time: s.current_time.max(0.0),
        was_playing: s.was_playing,
    }
}

pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Self { dir }
    }
    fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
    pub fn load_settings(&self) -> Settings {
        fs::read_to_string(self.file("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save_settings(&self, s: &Settings) -> Result<(), String> {
        let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
        fs::write(self.file("settings.json"), json).map_err(|e| e.to_string())
    }
    pub fn load_cookie(&self) -> Option<String> {
        fs::read_to_string(self.file("cookie.txt"))
            .ok()
            .filter(|s| !s.trim().is_empty())
    }
    pub fn save_cookie(&self, cookie: Option<&str>) -> Result<(), String> {
        let p = self.file("cookie.txt");
        match cookie {
            Some(c) => fs::write(p, c).map_err(|e| e.to_string()),
            None => {
                let _ = fs::remove_file(p);
                Ok(())
            }
        }
    }
    pub fn load_playback(&self) -> PlaybackSession {
        fs::read_to_string(self.file("playback.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .map(sanitize_session)
            .unwrap_or_default()
    }
    pub fn save_playback(&self, s: &PlaybackSession) -> Result<(), String> {
        let clean = sanitize_session(s.clone());
        let json = serde_json::to_string_pretty(&clean).map_err(|e| e.to_string())?;
        fs::write(self.file("playback.json"), json).map_err(|e| e.to_string())
    }

}
