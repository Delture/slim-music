//! 网易云元数据 / 音频磁盘缓存：stale-while-revalidate + 音频 LRU。
//!
//! 音频文件位于 `{app_data}/cache/audio/{id}_{quality}.{ext}`（移动端为系统 cache 目录）。
//! 按歌曲 id 与音质键控；查找时可复用更高或同等音质，绝不用更低音质顶替。
//! 先写临时 `.part` 再 rename；校验 Content-Length、最小体积、内容类型与音频魔数。
//! 超出上限按访问时间 LRU 淘汰。启动时清掉残留 `.part`。
use futures_util::StreamExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// 拒绝过小的响应，避免把错误页 / JSON 当成音频（约 0.5 秒 128k MP3）。
const MIN_AUDIO_BYTES: u64 = 8 * 1024;
const DOWNLOAD_TIMEOUT_SECS: u64 = 300;
const CONNECT_TIMEOUT_SECS: u64 = 20;
/// 下载时带浏览器头，避免网易云 CDN 对默认 reqwest UA 返回 403。
const DOWNLOAD_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
const DOWNLOAD_REFERER: &str = "https://music.163.com/";

/// 从低到高。查找缓存时只允许等于或高于用户所选音质。
pub const QUALITY_ORDER: &[&str] = &["standard", "higher", "exhigh", "lossless", "hires"];

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn quality_rank(q: &str) -> Option<usize> {
    QUALITY_ORDER.iter().position(|&x| x == q)
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

struct DownloadSlot {
    lock: tokio::sync::Mutex<()>,
    gen: AtomicU64,
}

#[derive(Clone)]
pub struct NetCache {
    root: PathBuf,
    slots: Arc<StdMutex<HashMap<String, Arc<DownloadSlot>>>>,
    /// 文件名 → 最近访问毫秒（不依赖文件系统 atime，Windows / noatime 也可靠）。
    lru: Arc<StdMutex<HashMap<String, u64>>>,
}

impl NetCache {
    pub fn new(app_data: PathBuf) -> Self {
        let root = app_data.join("cache");
        let _ = fs::create_dir_all(root.join("meta"));
        let _ = fs::create_dir_all(root.join("audio"));
        let cache = Self {
            root: root.clone(),
            slots: Arc::new(StdMutex::new(HashMap::new())),
            lru: Arc::new(StdMutex::new(load_lru_file(
                &root.join("audio").join(".lru.json"),
            ))),
        };
        cache.purge_partial_downloads();
        cache
    }

    /// 启动时清掉残留 *.part，避免半成品干扰下一次播放。
    pub fn purge_partial_downloads(&self) {
        let Ok(rd) = fs::read_dir(self.audio_dir()) else {
            return;
        };
        for e in rd.flatten() {
            let path = e.path();
            if is_part_file(&path) {
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

    fn slot(&self, key: &str) -> Arc<DownloadSlot> {
        let mut map = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(key.to_string())
            .or_insert_with(|| {
                Arc::new(DownloadSlot {
                    lock: tokio::sync::Mutex::new(()),
                    gen: AtomicU64::new(0),
                })
            })
            .clone()
    }

    fn bump_gen_for_id(&self, id: i64) {
        let prefix = format!("{id}_");
        let map = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        for (k, s) in map.iter() {
            if k.starts_with(&prefix) {
                s.gen.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn bump_gen(&self, key: &str) {
        let map = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = map.get(key) {
            s.gen.fetch_add(1, Ordering::SeqCst);
        }
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

    /// 已缓存且可用的音频路径（精确音质，否则可用更高音质；并 touch 访问时间做 LRU）。
    /// 忽略未下完的 `*.part`；空文件、过小或非音频内容视为损坏并删除。
    pub fn audio_cached_path(&self, id: i64, quality: &str) -> Option<PathBuf> {
        if let Some(p) = self.find_exact(id, quality) {
            if self.validate_or_remove(&p) {
                self.touch_lru(&p);
                return Some(p);
            }
        }
        self.find_higher(id, quality).and_then(|p| {
            if self.validate_or_remove(&p) {
                self.touch_lru(&p);
                Some(p)
            } else {
                None
            }
        })
    }

    fn find_exact(&self, id: i64, quality: &str) -> Option<PathBuf> {
        let dir = self.audio_dir();
        let prefix = format!("{id}_{quality}.");
        let Ok(rd) = fs::read_dir(&dir) else {
            return None;
        };
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&prefix) && !name.ends_with(".part") {
                return Some(e.path());
            }
        }
        None
    }

    /// 在精确音质缺失时，选用「刚好达到或高于」所选音质的最低一档（不用更低的顶替）。
    fn find_higher(&self, id: i64, quality: &str) -> Option<PathBuf> {
        let min_rank = quality_rank(quality)?;
        let dir = self.audio_dir();
        let Ok(rd) = fs::read_dir(&dir) else {
            return None;
        };
        let mut best: Option<(usize, PathBuf)> = None;
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            let Some((q, _)) = parse_audio_filename(&name, id) else {
                continue;
            };
            let Some(rank) = quality_rank(q) else {
                continue;
            };
            if rank < min_rank {
                continue;
            }
            // 精确档已在 find_exact 处理；这里只考虑更高档。
            if rank == min_rank {
                continue;
            }
            let path = e.path();
            match &best {
                None => best = Some((rank, path)),
                Some((br, _)) if rank < *br => best = Some((rank, path)),
                _ => {}
            }
        }
        best.map(|(_, p)| p)
    }

    fn validate_or_remove(&self, path: &Path) -> bool {
        match validate_audio_file(path) {
            Ok(()) => true,
            Err(_) => {
                let _ = fs::remove_file(path);
                false
            }
        }
    }

    /// 删除该曲目指定音质的缓存（含残留 `.part`），用于损坏条目或强制重新下载。
    pub fn invalidate_audio(&self, id: i64, quality: &str) {
        self.bump_gen(&format!("{id}_{quality}"));
        self.remove_matching(id, Some(quality));
    }

    /// 删除该曲目全部音质（刷新播放源时用，避免更高音质的损坏文件被反复命中）。
    pub fn invalidate_song_audio(&self, id: i64) {
        self.bump_gen_for_id(id);
        self.remove_matching(id, None);
    }

    fn remove_matching(&self, id: i64, quality: Option<&str>) {
        let dir = self.audio_dir();
        let Ok(rd) = fs::read_dir(&dir) else {
            return;
        };
        let mut removed = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            let Some((q, _)) = parse_audio_filename(&name, id) else {
                continue;
            };
            if quality.is_some_and(|want| q != want) {
                continue;
            }
            let path = e.path();
            if fs::remove_file(&path).is_ok() {
                removed.push(name.to_string());
            }
        }
        self.lru_remove_names(&removed);
    }

    fn touch_lru(&self, path: &Path) {
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            return;
        };
        {
            let mut lru = self.lru.lock().unwrap_or_else(|e| e.into_inner());
            lru.insert(name, now_millis());
            self.persist_lru(&lru);
        }
        touch_accessed(path);
    }

    fn lru_remove_names(&self, names: &[String]) {
        if names.is_empty() {
            return;
        }
        let mut lru = self.lru.lock().unwrap_or_else(|e| e.into_inner());
        for n in names {
            lru.remove(n);
        }
        self.persist_lru(&lru);
    }

    fn persist_lru(&self, lru: &HashMap<String, u64>) {
        let path = self.audio_dir().join(".lru.json");
        if let Ok(json) = serde_json::to_string(lru) {
            let _ = fs::write(path, json);
        }
    }

    pub async fn download_audio(
        &self,
        id: i64,
        quality: &str,
        url: &str,
        max_bytes: u64,
    ) -> Result<PathBuf, String> {
        let key = format!("{id}_{quality}");
        let slot = self.slot(&key);
        let _guard = slot.lock.lock().await;
        let start_gen = slot.gen.load(Ordering::SeqCst);

        // 持锁后再查：可能已被并发任务下完，或已有更高音质可复用。
        if let Some(p) = self.audio_cached_path(id, quality) {
            return Ok(p);
        }

        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .timeout(std::time::Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .get(url)
            .header(reqwest::header::USER_AGENT, DOWNLOAD_UA)
            .header(reqwest::header::REFERER, DOWNLOAD_REFERER)
            .header(reqwest::header::ACCEPT, "*/*")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("下载失败 HTTP {}", resp.status()));
        }
        let ctype = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        if is_rejected_content_type(&ctype) {
            return Err(format!("非音频 Content-Type: {ctype}"));
        }
        if let Some(len) = resp.content_length() {
            if len < MIN_AUDIO_BYTES {
                return Err(format!("响应过小：Content-Length={len}"));
            }
        }
        let ext = ext_from_ctype_or_url(&ctype, url);
        let dest = self.audio_dir().join(format!("{id}_{quality}.{ext}"));
        let tmp = self.audio_dir().join(format!(
            "{id}_{quality}.{ext}.{}.{}.part",
            std::process::id(),
            now_secs()
        ));

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
        if slot.gen.load(Ordering::SeqCst) != start_gen {
            let _ = fs::remove_file(&tmp);
            return Err("下载已取消".into());
        }
        if let Err(e) = validate_downloaded(&tmp, written, expected_len) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        if dest.exists() {
            let _ = fs::remove_file(&dest);
        }
        fs::rename(&tmp, &dest).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            e.to_string()
        })?;
        self.touch_lru(&dest);
        self.enforce_audio_limit(max_bytes);
        Ok(dest)
    }

    pub fn enforce_audio_limit(&self, max_bytes: u64) {
        if max_bytes == 0 {
            return;
        }
        let dir = self.audio_dir();
        let lru = self.lru.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut files: Vec<(PathBuf, String, u64, u64)> = Vec::new();
        let Ok(rd) = fs::read_dir(&dir) else {
            return;
        };
        for e in rd.flatten() {
            let path = e.path();
            if !is_cached_audio(&path) {
                continue;
            }
            let Ok(meta) = e.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            let accessed = lru.get(&name).copied().unwrap_or_else(|| {
                meta.accessed()
                    .or_else(|_| meta.modified())
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            });
            files.push((path, name, meta.len(), accessed));
        }
        let mut total: u64 = files.iter().map(|(_, _, n, _)| *n).sum();
        if total <= max_bytes {
            return;
        }
        files.sort_by_key(|(_, _, _, t)| *t); // oldest first
        let mut removed = Vec::new();
        for (path, name, len, _) in files {
            if total <= max_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(len);
                removed.push(name);
            }
        }
        drop(lru);
        self.lru_remove_names(&removed);
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
        // 作废所有在途下载，避免清完后又被 rename 写回。
        {
            let map = self.slots.lock().unwrap_or_else(|e| e.into_inner());
            for s in map.values() {
                s.gen.fetch_add(1, Ordering::SeqCst);
            }
        }
        for sub in ["meta", "audio"] {
            let p = self.root.join(sub);
            if p.exists() {
                fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
            }
            fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        }
        {
            let mut lru = self.lru.lock().unwrap_or_else(|e| e.into_inner());
            lru.clear();
        }
        Ok(())
    }
}

fn is_part_file(path: &Path) -> bool {
    path.extension().and_then(|x| x.to_str()) == Some("part")
        || path
            .file_name()
            .and_then(|x| x.to_str())
            .is_some_and(|n| n.ends_with(".part"))
}

fn is_lru_index(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some(".lru.json")
}

/// 计入 LRU / 用量的完整音频缓存文件（排除 .part 与索引）。
fn is_cached_audio(path: &Path) -> bool {
    !is_part_file(path) && !is_lru_index(path)
}

fn load_lru_file(path: &Path) -> HashMap<String, u64> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `{id}_{quality}.{ext}`，也接受清扫时的 `{id}_{quality}.{ext}.{pid}.{ts}.part`。
/// 要求 `{id}_` 后面紧跟已知音质名，避免 `42` 误匹配 `420_exhigh.mp3`。
fn parse_audio_filename(name: &str, id: i64) -> Option<(&str, &str)> {
    let rest = name.strip_prefix(&format!("{id}_"))?;
    let rest = rest.strip_suffix(".part").unwrap_or(rest);
    let (quality, ext) = rest.split_once('.')?;
    if quality.is_empty() || ext.is_empty() {
        return None;
    }
    if quality_rank(quality).is_none() {
        return None;
    }
    Some((quality, ext))
}

fn is_rejected_content_type(ctype: &str) -> bool {
    let c = ctype.to_ascii_lowercase();
    if c.is_empty() || c.contains("octet-stream") || c.contains("audio/") {
        return false;
    }
    c.contains("json")
        || c.contains("html")
        || c.contains("javascript")
        || c.contains("xml")
        || c.contains("text/plain")
        || c.contains("text/css")
}

fn looks_like_text_error(buf: &[u8]) -> bool {
    let start = buf
        .iter()
        .position(|&b| !b.is_ascii_whitespace())
        .map(|i| &buf[i..])
        .unwrap_or(buf);
    start.starts_with(b"<") || start.starts_with(b"{") || start.starts_with(b"[")
}

fn looks_like_audio(buf: &[u8]) -> bool {
    if buf.len() < 12 {
        return false;
    }
    if buf.starts_with(b"ID3") || buf.starts_with(b"fLaC") || buf.starts_with(b"OggS") {
        return true;
    }
    if buf.starts_with(b"APETAGEX") {
        return true;
    }
    // MP3 / AAC ADTS 帧同步
    if buf[0] == 0xFF && (buf[1] & 0xE0) == 0xE0 {
        return true;
    }
    // MP4 / M4A
    if &buf[4..8] == b"ftyp" {
        return true;
    }
    // WAV
    if buf.starts_with(b"RIFF") && &buf[8..12] == b"WAVE" {
        return true;
    }
    false
}

fn peek_head(path: &Path, n: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut f = fs::File::open(path).ok()?;
    let mut buf = vec![0u8; n];
    let k = f.read(&mut buf).ok()?;
    buf.truncate(k);
    Some(buf)
}

fn validate_audio_file(path: &Path) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("不是文件".into());
    }
    let len = meta.len();
    if len == 0 {
        return Err("文件为空".into());
    }
    if len < MIN_AUDIO_BYTES {
        return Err(format!("文件过小：{len} 字节"));
    }
    let head = peek_head(path, 16).unwrap_or_default();
    if looks_like_text_error(&head) {
        return Err("文件内容不是音频".into());
    }
    if !looks_like_audio(&head) {
        // 体积足够且不是文本错误页时放行（少见容器），否则拒绝。
        if len < 32 * 1024 {
            return Err("无法识别音频格式".into());
        }
    }
    Ok(())
}

fn validate_downloaded(tmp: &Path, written: u64, expected_len: Option<u64>) -> Result<(), String> {
    if written == 0 {
        return Err("下载文件为空".into());
    }
    if written < MIN_AUDIO_BYTES {
        return Err(format!("下载过小：{written} 字节"));
    }
    if let Some(exp) = expected_len {
        if written != exp {
            return Err(format!(
                "下载不完整：Content-Length={exp}，实际写入={written}"
            ));
        }
    }
    validate_audio_file(tmp)
}

fn touch_accessed(path: &Path) {
    let Ok(f) = fs::File::open(path) else {
        return;
    };
    let times = fs::FileTimes::new().set_accessed(SystemTime::now());
    let _ = f.set_times(times);
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
    let path = u.split(['?', '#']).next().unwrap_or(&u);
    if path.ends_with(".flac") || path.contains(".flac") {
        return "flac";
    }
    if path.ends_with(".m4a")
        || path.ends_with(".mp4")
        || path.contains(".m4a")
        || path.contains(".mp4")
    {
        return "m4a";
    }
    if path.contains(".ogg") {
        return "ogg";
    }
    "mp3"
}

fn dir_size(path: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(path) else {
        return 0;
    };
    rd.flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

fn dir_size_count(path: &Path) -> (u64, u32) {
    let Ok(rd) = fs::read_dir(path) else {
        return (0, 0);
    };
    let mut bytes = 0u64;
    let mut n = 0u32;
    for e in rd.flatten() {
        if !is_cached_audio(&e.path()) {
            continue;
        }
        if let Ok(m) = e.metadata() {
            if m.is_file() {
                bytes += m.len();
                n += 1;
            }
        }
    }
    (bytes, n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn tmp_cache() -> (NetCache, tempfile_dir::Tmp) {
        let t = tempfile_dir::Tmp::new();
        (NetCache::new(t.path.clone()), t)
    }

    /// 极小临时目录工具，避免额外 dev-dependency。
    mod tempfile_dir {
        use std::path::PathBuf;
        pub struct Tmp {
            pub path: PathBuf,
        }
        impl Tmp {
            pub fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "slim-music-cache-test-{}-{}",
                    std::process::id(),
                    super::now_secs_unique()
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self { path }
            }
        }
        impl Drop for Tmp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    fn now_secs_unique() -> u128 {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        t + u128::from(N.fetch_add(1, Ordering::SeqCst))
    }

    fn fake_mp3(n: usize) -> Vec<u8> {
        let mut v = b"ID3".to_vec();
        v.resize(n.max(12), 0x00);
        v
    }

    fn write_audio(cache: &NetCache, id: i64, quality: &str, ext: &str, n: usize) -> PathBuf {
        let p = cache.audio_dir().join(format!("{id}_{quality}.{ext}"));
        fs::write(&p, fake_mp3(n)).unwrap();
        p
    }

    #[test]
    fn quality_rank_order() {
        assert!(quality_rank("standard").unwrap() < quality_rank("higher").unwrap());
        assert!(quality_rank("higher").unwrap() < quality_rank("exhigh").unwrap());
        assert!(quality_rank("exhigh").unwrap() < quality_rank("lossless").unwrap());
        assert!(quality_rank("lossless").unwrap() < quality_rank("hires").unwrap());
        assert!(quality_rank("foo").is_none());
    }

    #[test]
    fn parse_filename_and_ignore_part() {
        assert_eq!(
            parse_audio_filename("42_exhigh.mp3", 42),
            Some(("exhigh", "mp3"))
        );
        assert_eq!(
            parse_audio_filename("42_exhigh.mp3.123.99.part", 42).map(|(q, _)| q),
            Some("exhigh")
        );
        // 不会把 420_exhigh 当成 42_
        assert!(parse_audio_filename("420_exhigh.mp3", 42).is_none());
        assert!(parse_audio_filename("42_unknown.mp3", 42).is_none());
    }

    #[test]
    fn looks_like_audio_and_errors() {
        assert!(looks_like_audio(&fake_mp3(16)));
        assert!(looks_like_audio(b"fLaC........"));
        assert!(looks_like_audio(b"OggS........"));
        let mut m4a = vec![0u8; 16];
        m4a[4..8].copy_from_slice(b"ftyp");
        assert!(looks_like_audio(&m4a));
        assert!(looks_like_text_error(b"  {\"code\":404}"));
        assert!(looks_like_text_error(b"<html>err</html>"));
        assert!(is_rejected_content_type("application/json"));
        assert!(is_rejected_content_type("text/html; charset=utf-8"));
        assert!(!is_rejected_content_type("audio/mpeg"));
        assert!(!is_rejected_content_type("application/octet-stream"));
        assert!(!is_rejected_content_type(""));
    }

    #[test]
    fn exact_hit_and_rejects_tiny_or_html() {
        let (c, _t) = tmp_cache();
        write_audio(&c, 7, "exhigh", "mp3", 16 * 1024);
        let hit = c.audio_cached_path(7, "exhigh").unwrap();
        assert!(hit.to_string_lossy().ends_with("7_exhigh.mp3"));

        // 过小
        let tiny = c.audio_dir().join("8_exhigh.mp3");
        fs::write(&tiny, fake_mp3(100)).unwrap();
        assert!(c.audio_cached_path(8, "exhigh").is_none());
        assert!(!tiny.exists(), "损坏文件应被删除");

        // HTML 错误页
        let html = c.audio_dir().join("9_exhigh.mp3");
        fs::write(&html, b"<html>forbidden</html>".repeat(800)).unwrap();
        assert!(c.audio_cached_path(9, "exhigh").is_none());
        assert!(!html.exists());
    }

    #[test]
    fn higher_quality_reused_never_lower() {
        let (c, _t) = tmp_cache();
        write_audio(&c, 1, "exhigh", "mp3", 16 * 1024);
        // 用户选 standard / higher，可复用 exhigh
        let p = c.audio_cached_path(1, "standard").unwrap();
        assert!(p.to_string_lossy().contains("1_exhigh."));
        let p2 = c.audio_cached_path(1, "higher").unwrap();
        assert!(p2.to_string_lossy().contains("1_exhigh."));
        // 用户选 lossless，不能用 exhigh
        assert!(c.audio_cached_path(1, "lossless").is_none());
        assert!(c.audio_cached_path(1, "hires").is_none());

        // 同时有 higher 与 hires 时，选 standard 应取刚好更高的 higher，而不是跨到 hires
        write_audio(&c, 2, "higher", "mp3", 16 * 1024);
        write_audio(&c, 2, "hires", "flac", 16 * 1024);
        let p = c.audio_cached_path(2, "standard").unwrap();
        assert!(p.to_string_lossy().contains("2_higher."));
    }

    #[test]
    fn ignore_part_and_prefix_collision() {
        let (c, _t) = tmp_cache();
        fs::write(c.audio_dir().join("5_exhigh.mp3.part"), fake_mp3(16 * 1024)).unwrap();
        assert!(c.audio_cached_path(5, "exhigh").is_none());
        write_audio(&c, 51, "exhigh", "mp3", 16 * 1024);
        assert!(c.audio_cached_path(5, "exhigh").is_none());
        assert!(c.audio_cached_path(51, "exhigh").is_some());
    }

    #[test]
    fn lru_evicts_oldest() {
        let (c, _t) = tmp_cache();
        let a = write_audio(&c, 1, "exhigh", "mp3", 20 * 1024);
        thread::sleep(std::time::Duration::from_millis(20));
        let b = write_audio(&c, 2, "exhigh", "mp3", 20 * 1024);
        thread::sleep(std::time::Duration::from_millis(20));
        // 访问 a，使其比 b 新
        assert!(c.audio_cached_path(1, "exhigh").is_some());
        thread::sleep(std::time::Duration::from_millis(20));
        // 上限刚好只容下一个 20KB 文件（再加一点余量不够两个）
        c.enforce_audio_limit(28 * 1024);
        assert!(a.exists(), "最近访问的应保留");
        assert!(!b.exists(), "更久未访问的应淘汰");
    }

    #[test]
    fn stats_skip_part_and_clear_drops_audio() {
        let (c, _t) = tmp_cache();
        write_audio(&c, 1, "exhigh", "mp3", 16 * 1024);
        fs::write(c.audio_dir().join("1_exhigh.mp3.part"), fake_mp3(16 * 1024)).unwrap();
        let s = c.stats();
        assert_eq!(s.audio_files, 1);
        assert!(s.audio_bytes >= 16 * 1024);
        c.clear().unwrap();
        let s = c.stats();
        assert_eq!(s.audio_files, 0);
        assert_eq!(s.audio_bytes, 0);
        assert!(c.audio_cached_path(1, "exhigh").is_none());
    }

    #[test]
    fn invalidate_song_removes_all_qualities() {
        let (c, _t) = tmp_cache();
        write_audio(&c, 3, "standard", "mp3", 16 * 1024);
        write_audio(&c, 3, "hires", "flac", 16 * 1024);
        write_audio(&c, 4, "exhigh", "mp3", 16 * 1024);
        c.invalidate_song_audio(3);
        assert!(c.audio_cached_path(3, "standard").is_none());
        assert!(c.audio_cached_path(3, "hires").is_none());
        assert!(c.audio_cached_path(4, "exhigh").is_some());
    }

    fn serve_bytes(status: &str, ctype: &str, body: Vec<u8>, extra: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let status = status.to_string();
        let ctype = ctype.to_string();
        let extra = extra.to_string();
        thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let header = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
                body.len()
            );
            let _ = s.write_all(header.as_bytes());
            let _ = s.write_all(&body);
        });
        format!("http://{addr}/song.mp3")
    }

    #[tokio::test]
    async fn download_saves_valid_mp3_and_rejects_html() {
        let (c, _t) = tmp_cache();
        let body = fake_mp3(16 * 1024);
        let url = serve_bytes("200 OK", "audio/mpeg", body, "");
        let p = c
            .download_audio(11, "exhigh", &url, 100 * 1024 * 1024)
            .await
            .expect("download ok");
        assert!(p.ends_with("11_exhigh.mp3"));
        assert!(c.audio_cached_path(11, "exhigh").is_some());

        let url = serve_bytes("200 OK", "text/html", b"<html>error</html>".repeat(600), "");
        let err = c
            .download_audio(12, "exhigh", &url, 100 * 1024 * 1024)
            .await
            .unwrap_err();
        assert!(
            err.contains("Content-Type") || err.contains("不是音频") || err.contains("过小"),
            "unexpected: {err}"
        );
        assert!(c.audio_cached_path(12, "exhigh").is_none());
    }

    #[tokio::test]
    async fn download_rejects_tiny_and_http_error() {
        let (c, _t) = tmp_cache();
        let url = serve_bytes("200 OK", "audio/mpeg", fake_mp3(100), "");
        assert!(c
            .download_audio(13, "exhigh", &url, 100 * 1024 * 1024)
            .await
            .is_err());
        assert!(c.audio_cached_path(13, "exhigh").is_none());

        let url = serve_bytes("403 Forbidden", "text/html", b"<h1>no</h1>".to_vec(), "");
        let err = c
            .download_audio(14, "exhigh", &url, 100 * 1024 * 1024)
            .await
            .unwrap_err();
        assert!(err.contains("403"), "{err}");
        assert!(c.audio_cached_path(14, "exhigh").is_none());
    }

    #[tokio::test]
    async fn download_reuses_higher_without_new_file() {
        let (c, _t) = tmp_cache();
        write_audio(&c, 20, "hires", "flac", 16 * 1024);
        // 即使 URL 不可达，也应直接复用更高音质
        let p = c
            .download_audio(20, "standard", "http://127.0.0.1:1/nope", 100 * 1024 * 1024)
            .await
            .unwrap();
        assert!(p.to_string_lossy().contains("20_hires."));
    }
}
