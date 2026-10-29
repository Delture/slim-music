//! 设置与登录 cookie 持久化（应用数据目录下的 JSON 文件）。
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

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
}
