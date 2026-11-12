//! 桌面端：监听已添加的本地音乐文件夹，防抖后增量更新索引。
//!
//! 用成熟的 `notify` + `notify-debouncer-mini`：事件先在静默约 1.2s 后合并，
//! 再交给 [`crate::local::Library::apply_paths`]，避免每个小变动都整库重扫或刷 UI。
use crate::local::{self, ApplyResult};
use crate::AppState;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, DebouncedEventKind, Debouncer};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const DEBOUNCE: Duration = Duration::from_millis(1200);

struct WatchInner {
    debouncer: Debouncer<RecommendedWatcher>,
    folders: Vec<String>,
}

static WATCH: Mutex<Option<WatchInner>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<WatchInner>> {
    WATCH.lock().unwrap_or_else(|e| e.into_inner())
}

/// 在 `setup` 里调用一次：启动监听线程并订阅当前设置中的文件夹。
pub fn start(app: AppHandle) {
    let (tx, rx) = mpsc::channel::<DebounceEventResult>();
    let debouncer = match new_debouncer(DEBOUNCE, tx) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("本地文件夹监听不可用: {e}");
            return;
        }
    };
    {
        let mut g = lock();
        *g = Some(WatchInner {
            debouncer,
            folders: Vec::new(),
        });
    }
    let folders = app
        .state::<AppState>()
        .settings
        .blocking_lock()
        .local_folders
        .clone();
    set_folders(&folders);

    let _ = std::thread::Builder::new()
        .name("local-watch".into())
        .spawn(move || worker(app, rx));
}

/// 设置变更后更新监听路径（取消旧的，订阅新的）。
pub fn set_folders(folders: &[String]) {
    let mut g = lock();
    let Some(inner) = g.as_mut() else {
        return;
    };
    for old in &inner.folders {
        let _ = inner.debouncer.watcher().unwatch(Path::new(old));
    }
    let mut ok = Vec::new();
    for f in folders {
        let p = Path::new(f);
        if !p.is_dir() {
            continue;
        }
        match inner.debouncer.watcher().watch(p, RecursiveMode::Recursive) {
            Ok(()) => ok.push(f.clone()),
            Err(e) => eprintln!("无法监听本地文件夹 {f}: {e}"),
        }
    }
    inner.folders = ok;
}

fn worker(app: AppHandle, rx: Receiver<DebounceEventResult>) {
    while let Ok(res) = rx.recv() {
        let mut paths: Vec<PathBuf> = match res {
            Ok(events) => collect_paths(events),
            Err(e) => {
                eprintln!("本地文件夹监听: {e}");
                Vec::new()
            }
        };
        while let Ok(more) = rx.try_recv() {
            match more {
                Ok(events) => paths.extend(collect_paths(events)),
                Err(e) => eprintln!("本地文件夹监听: {e}"),
            }
        }
        paths.retain(|p| is_interesting(p));
        if paths.is_empty() {
            continue;
        }
        paths.sort();
        paths.dedup();

        let state = app.state::<AppState>();
        let folders = state.settings.blocking_lock().local_folders.clone();
        let result: ApplyResult = state.library.apply_paths(&folders, &paths);
        if result.library_changed || !result.lyrics_changed.is_empty() {
            let _ = app.emit("local-library-updated", &result);
        }
    }
}

fn collect_paths(events: Vec<notify_debouncer_mini::DebouncedEvent>) -> Vec<PathBuf> {
    events
        .into_iter()
        // 仍在连续写入（拷贝大文件）时先跳过，等静默后的最终 Any
        .filter(|e| e.kind != DebouncedEventKind::AnyContinuous)
        .map(|e| e.path)
        .collect()
}

fn is_interesting(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.is_empty() {
        return false;
    }
    if name.starts_with('.') || name.eq_ignore_ascii_case("Thumbs.db") || name.eq_ignore_ascii_case("desktop.ini")
    {
        return false;
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        ext.as_str(),
        "tmp" | "part" | "crdownload" | "download" | "partial" | "bak" | "old"
    ) {
        return false;
    }
    // 已删除的路径无法判断类型，交给 apply_paths 对照索引处理
    if !p.exists() {
        return true;
    }
    if p.is_dir() {
        return true;
    }
    local::has_audio_ext(p) || local::is_lrc_path(p) || local::is_folder_cover_path(p)
}
