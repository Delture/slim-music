//! 网易云元数据 / 音频磁盘缓存：stale-while-revalidate + 音频 LRU。
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use futures_util::StreamExt;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<T> {
    saved_at: u64,
    data: T,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheHit<T> {
    pub data: T,
    pub from_cache: bool,
    /// 缓存年龄（秒）；网络新鲜数据为 0
    pub age_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub total_bytes: u64,
    pub meta_bytes: u64,
    pub audio_bytes: u64,
    pub audio_files: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySrc {
    /// "file" | "url"
    pub kind: String,
    pub value: String,
    pub from_cache: bool,
}

#[derive(Clone)]
pub struct NetCache {
    root: PathBuf,
}

impl NetCache {
    pub fn new(app_data: PathBuf) -> Self {
        let root = app_data.join("cache");
        let _ = fs::create_dir_all(root.join("meta"));
        let _ = fs::create_dir_all(root.join("audio"));
        let cache = Self { root };
        cache.purge_partial_downloads();
        cache
    }

    /// 启动时清掉残留 *.part，避免半成品干扰下一次播放。
    pub fn purge_partial_downloads(&self) {
        let Ok(rd) = fs::read_dir(self.audio_dir()) else { return };
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) == Some("part") {
                let _ = fs::remove_file(path);
            }
        }
    }

    fn meta_path(&self, key: &str) -> PathBuf {
        self.root.join("meta").join(format!("{key}.json"))
    }

    fn audio_dir(&self) -> PathBuf {
        self.root.join("audio")
    }

    pub fn read_json<T: DeserializeOwned>(&self, key: &str) -> Option<CacheHit<T>> {
        let path = self.meta_path(key);
        let raw = fs::read_to_string(path).ok()?;
        let env: Envelope<T> = serde_json::from_str(&raw).ok()?;
        let age = now_secs().saturating_sub(env.saved_at);
        Some(CacheHit {
            data: env.data,
            from_cache: true,
            age_secs: age,
        })
    }

    pub fn write_json<T: Serialize>(&self, key: &str, data: &T) -> Result<(), String> {
        let env = Envelope {
            saved_at: now_secs(),
            data,
        };
        let json = serde_json::to_string(&env).map_err(|e| e.to_string())?;
        fs::write(self.meta_path(key), json).map_err(|e| e.to_string())
    }

    /// 已缓存的音频文件路径（并 touch 访问时间做 LRU）。
    /// 忽略未下完的 `*.part`，否则第一次播放会把半成品当缓存，解码失败。
    /// 空文件视为损坏并删除。
    pub fn audio_cached_path(&self, id: i64, quality: &str) -> Option<PathBuf> {
        let dir = self.audio_dir();
        let prefix = format!("{id}_{quality}.");
        let mut found: Option<PathBuf> = None;
        if let Ok(rd) = fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with(&prefix) {
                    continue;
                }
                if name.ends_with(".part") {
                    continue;
                }
                found = Some(e.path());
                break;
            }
        }
        if let Some(ref p) = found {
            let len = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            if len == 0 {
                let _ = fs::remove_file(p);
                return None;
            }
            let _ = filetime_touch(p);
        }
        found
    }

    /// 删除该曲目指定音质的缓存（含残留 `.part`），用于损坏条目或强制重新下载。
    pub fn invalidate_audio(&self, id: i64, quality: &str) {
        let dir = self.audio_dir();
        let prefix = format!("{id}_{quality}.");
        let Ok(rd) = fs::read_dir(&dir) else { return };
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&prefix) {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    pub async fn download_audio(
        &self,
        id: i64,
        quality: &str,
        url: &str,
        max_bytes: u64,
    ) -> Result<PathBuf, String> {
        // 已有则直接返回
        if let Some(p) = self.audio_cached_path(id, quality) {
            return Ok(p);
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("下载失败 HTTP {}", resp.status()));
        }
        let ctype = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let ext = ext_from_ctype_or_url(&ctype, url);
        let dest = self.audio_dir().join(format!("{id}_{quality}.{ext}"));
        let tmp = self.audio_dir().join(format!("{id}_{quality}.{ext}.part"));

        let expected_len = resp.content_length();
        let mut stream = resp.bytes_stream();
        let mut file = tokio::fs::File::create(&tmp)
            .await
            .map_err(|e| e.to_string())?;
        use tokio::io::AsyncWriteExt;
        let mut written: u64 = 0;
        let write_result: Result<(), String> = async {
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| e.to_string())?;
                written += chunk.len() as u64;
                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            }
            file.flush().await.map_err(|e| e.to_string())?;
            Ok(())
        }
        .await;
        drop(file);
        if let Err(e) = write_result {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        if written == 0 {
            let _ = fs::remove_file(&tmp);
            return Err("下载文件为空".into());
        }
        if let Some(exp) = expected_len {
            if written != exp {
                let _ = fs::remove_file(&tmp);
                return Err(format!(
                    "下载不完整：Content-Length={exp}，实际写入={written}"
                ));
            }
        }
        fs::rename(&tmp, &dest).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            e.to_string()
        })?;
        self.enforce_audio_limit(max_bytes);
        Ok(dest)
    }

    pub fn enforce_audio_limit(&self, max_bytes: u64) {
        if max_bytes == 0 {
            return;
        }
        let dir = self.audio_dir();
        let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
        let Ok(rd) = fs::read_dir(&dir) else { return };
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) == Some("part") {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let accessed = meta.accessed().or_else(|_| meta.modified()).unwrap_or(UNIX_EPOCH);
            files.push((path, meta.len(), accessed));
        }
        let mut total: u64 = files.iter().map(|(_, n, _)| *n).sum();
        if total <= max_bytes {
            return;
        }
        files.sort_by_key(|(_, _, t)| *t); // oldest first
        for (path, len, _) in files {
            if total <= max_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(len);
            }
        }
    }

    pub fn stats(&self) -> CacheStats {
        let meta_bytes = dir_size(&self.root.join("meta"));
        let (audio_bytes, audio_files) = dir_size_count(&self.audio_dir());
        CacheStats {
            total_bytes: meta_bytes + audio_bytes,
            meta_bytes,
            audio_bytes,
            audio_files,
        }
    }

    pub fn clear(&self) -> Result<(), String> {
        for sub in ["meta", "audio"] {
            let p = self.root.join(sub);
            if p.exists() {
                fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
            }
            fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn filetime_touch(path: &Path) {
    let now = filetime_now();
    let _ = set_file_times(path, now);
}

fn filetime_now() -> SystemTime {
    SystemTime::now()
}

fn set_file_times(path: &Path, _t: SystemTime) {
    // Best-effort touch so LRU can use mtime/atime.
    let _ = fs::OpenOptions::new().append(true).open(path);
}

fn ext_from_ctype_or_url(ctype: &str, url: &str) -> &'static str {
    let c = ctype.to_ascii_lowercase();
    if c.contains("flac") {
        return "flac";
    }
    if c.contains("mp4") || c.contains("m4a") || c.contains("aac") {
        return "m4a";
    }
    if c.contains("mpeg") || c.contains("mp3") {
        return "mp3";
    }
    if c.contains("ogg") {
        return "ogg";
    }
    if c.contains("wav") {
        return "wav";
    }
    let u = url.to_ascii_lowercase();
    if u.contains(".flac") {
        return "flac";
    }
    if u.contains(".m4a") || u.contains(".mp4") {
        return "m4a";
    }
    if u.contains(".ogg") {
        return "ogg";
    }
    "mp3"
}

fn dir_size(path: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(path) else { return 0 };
    rd.flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

fn dir_size_count(path: &Path) -> (u64, u32) {
    let Ok(rd) = fs::read_dir(path) else { return (0, 0) };
    let mut bytes = 0u64;
    let mut n = 0u32;
    for e in rd.flatten() {
        if let Ok(m) = e.metadata() {
            if m.is_file() {
                bytes += m.len();
                n += 1;
            }
        }
    }
    (bytes, n)
}
