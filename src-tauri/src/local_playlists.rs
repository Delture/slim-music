//! 本地播放列表（Walkman 式）：与网易云歌单、当前播放队列都无关，JSON 持久化。
use crate::model::Track;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPlaylist {
    pub id: String,
    pub name: String,
    pub tracks: Vec<Track>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FileData {
    playlists: Vec<LocalPlaylist>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn new_id() -> String {
    format!("lp-{}-{:04}", now_ms(), now_ms() % 9973)
}

pub struct LocalPlaylistStore {
    path: PathBuf,
}

impl LocalPlaylistStore {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Self {
            path: dir.join("local_playlists.json"),
        }
    }

    fn load(&self) -> FileData {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, data: &FileData) -> Result<(), String> {
        let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
        fs::write(&self.path, json).map_err(|e| e.to_string())
    }

    pub fn list(&self) -> Vec<LocalPlaylist> {
        self.load().playlists
    }

    pub fn create(&self, name: String) -> Result<LocalPlaylist, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("名称不能为空".into());
        }
        let mut data = self.load();
        let pl = LocalPlaylist {
            id: new_id(),
            name: name.to_string(),
            tracks: vec![],
            updated_at: now_ms(),
        };
        data.playlists.push(pl.clone());
        self.save(&data)?;
        Ok(pl)
    }

    pub fn rename(&self, id: &str, name: String) -> Result<LocalPlaylist, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("名称不能为空".into());
        }
        let mut data = self.load();
        let pl = data
            .playlists
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| "播放列表不存在".to_string())?;
        pl.name = name.to_string();
        pl.updated_at = now_ms();
        let out = pl.clone();
        self.save(&data)?;
        Ok(out)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut data = self.load();
        let before = data.playlists.len();
        data.playlists.retain(|p| p.id != id);
        if data.playlists.len() == before {
            return Err("播放列表不存在".into());
        }
        self.save(&data)
    }

    pub fn get(&self, id: &str) -> Result<LocalPlaylist, String> {
        self.load()
            .playlists
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| "播放列表不存在".into())
    }

    /// 整表替换曲目（用于排序 / 批量编辑）
    pub fn set_tracks(&self, id: &str, tracks: Vec<Track>) -> Result<LocalPlaylist, String> {
        let mut data = self.load();
        let pl = data
            .playlists
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| "播放列表不存在".to_string())?;
        pl.tracks = tracks;
        pl.updated_at = now_ms();
        let out = pl.clone();
        self.save(&data)?;
        Ok(out)
    }

    /// 追加一首（同 id 可重复添加，Walkman 式允许重复）
    pub fn add_track(&self, id: &str, track: Track) -> Result<LocalPlaylist, String> {
        let mut data = self.load();
        let pl = data
            .playlists
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| "播放列表不存在".to_string())?;
        pl.tracks.push(track);
        pl.updated_at = now_ms();
        let out = pl.clone();
        self.save(&data)?;
        Ok(out)
    }

    /// 按出现位置删除一首（index）；若未传 index 则删掉第一个匹配 track.id
    pub fn remove_track(
        &self,
        id: &str,
        track_id: &str,
        index: Option<usize>,
    ) -> Result<LocalPlaylist, String> {
        let mut data = self.load();
        let pl = data
            .playlists
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| "播放列表不存在".to_string())?;
        if let Some(i) = index {
            if i < pl.tracks.len() && pl.tracks[i].id == track_id {
                pl.tracks.remove(i);
            } else if i < pl.tracks.len() {
                pl.tracks.remove(i);
            } else {
                return Err("索引无效".into());
            }
        } else if let Some(i) = pl.tracks.iter().position(|t| t.id == track_id) {
            pl.tracks.remove(i);
        } else {
            return Err("列表中没有这首歌".into());
        }
        pl.updated_at = now_ms();
        let out = pl.clone();
        self.save(&data)?;
        Ok(out)
    }

    pub fn move_track(&self, id: &str, from: usize, to: usize) -> Result<LocalPlaylist, String> {
        let mut data = self.load();
        let pl = data
            .playlists
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| "播放列表不存在".to_string())?;
        let len = pl.tracks.len();
        if from >= len || to >= len {
            return Err("索引无效".into());
        }
        let t = pl.tracks.remove(from);
        pl.tracks.insert(to, t);
        pl.updated_at = now_ms();
        let out = pl.clone();
        self.save(&data)?;
        Ok(out)
    }
}
