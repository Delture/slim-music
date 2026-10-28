//! 统一的数据模型：本地与网易云歌曲共用一个 Track。
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Local,
    Netease,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// 全局唯一 id："ncm:<歌曲id>" 或 "local:<文件路径>"
    pub id: String,
    pub source: Source,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_ms: u64,
    /// 封面 URL（网易云）；本地暂无（TODO）
    pub cover: Option<String>,
    /// 本地文件绝对路径
    pub path: Option<String>,
    /// 网易云歌曲 id
    pub ncm_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub cover: Option<String>,
    pub track_count: i64,
    /// 是否为“我喜欢的音乐”（用户创建的第一个歌单，specialType == 5）
    pub is_liked: bool,
    pub creator_id: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub user_id: i64,
    pub nickname: String,
    pub avatar: Option<String>,
}
