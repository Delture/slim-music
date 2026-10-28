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
}

impl Default for Settings {
    fn default() -> Self {
        Self { quality: "exhigh".into(), local_folders: vec![], volume: 0.8 }
    }
}

pub struct Store {
    dir: PathBuf,
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
        fs::read_to_string(self.file("cookie.txt")).ok().filter(|s| !s.trim().is_empty())
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
