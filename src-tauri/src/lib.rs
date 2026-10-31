//! Tauri 入口：所有前端调用都经由这里的 #[tauri::command]。
mod cache;
mod local;
mod local_playlists;
mod mobile;
mod model;
mod ncm;
mod settings;

use model::{LyricPayload, Playlist, Profile, Track};
use settings::{PlaybackSession, Settings, Store, QUALITY_LEVELS};
#[cfg(desktop)]
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, WindowEvent,
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

struct AppState {
    store: Store,
    cache: cache::NetCache,
    local_pls: local_playlists::LocalPlaylistStore,
    cookie: Mutex<Option<String>>,
    settings: Mutex<Settings>,
}

type R<T> = Result<T, String>;

#[derive(serde::Serialize)]
struct QrKeyOut {
    key: String,
    svg: String,
}

fn cookie(state: &AppState) -> impl std::future::Future<Output = Option<String>> + '_ {
    async { state.cookie.lock().await.clone() }
}

#[tauri::command]
async fn login_qr_key() -> R<QrKeyOut> {
    let k = ncm::qr_key().await?;
    Ok(QrKeyOut { key: k.key, svg: k.svg })
}

#[tauri::command]
async fn login_qr_check(state: State<'_, AppState>, key: String) -> R<i64> {
    let (code, cookie) = ncm::qr_check(&key).await?;
    if let Some(c) = cookie {
        state.store.save_cookie(Some(&c))?;
        *state.cookie.lock().await = Some(c);
    }
    Ok(code)
}

#[tauri::command]
async fn logout(state: State<'_, AppState>) -> R<()> {
    state.store.save_cookie(None)?;
    *state.cookie.lock().await = None;
    Ok(())
}

#[tauri::command]
async fn get_profile(state: State<'_, AppState>) -> R<Option<Profile>> {
    ncm::profile(&cookie(&state).await).await
}

#[tauri::command]
async fn daily_recommend(state: State<'_, AppState>) -> R<Vec<Track>> {
    ncm::daily(&cookie(&state).await).await
}

#[tauri::command]
async fn search(state: State<'_, AppState>, keywords: String, offset: u32) -> R<Vec<Track>> {
    ncm::search(&cookie(&state).await, &keywords, offset).await
}

#[tauri::command]
async fn my_playlists(
    state: State<'_, AppState>,
    refresh: Option<bool>,
) -> R<cache::CacheHit<Vec<Playlist>>> {
    let refresh = refresh.unwrap_or(false);
    let enabled = state.settings.lock().await.cache_enabled;
    if enabled && !refresh {
        if let Some(hit) = state.cache.read_json::<Vec<Playlist>>("playlists") {
            return Ok(hit);
        }
    }
    let c = cookie(&state).await;
    let p = ncm::profile(&c).await?.ok_or("请先登录")?;
    match ncm::user_playlists(&c, p.user_id).await {
        Ok(list) => {
            if enabled {
                let _ = state.cache.write_json("playlists", &list);
            }
            Ok(cache::CacheHit {
                data: list,
                from_cache: false,
                age_secs: 0,
            })
        }
        Err(e) => {
            if let Some(hit) = state.cache.read_json::<Vec<Playlist>>("playlists") {
                return Ok(hit);
            }
            Err(e)
        }
    }
}

#[tauri::command]
async fn playlist_tracks(
    state: State<'_, AppState>,
    id: i64,
    refresh: Option<bool>,
) -> R<cache::CacheHit<Vec<Track>>> {
    let refresh = refresh.unwrap_or(false);
    let key = format!("playlist_{id}");
    let enabled = state.settings.lock().await.cache_enabled;
    if enabled && !refresh {
        if let Some(hit) = state.cache.read_json::<Vec<Track>>(&key) {
            return Ok(hit);
        }
    }
    let c = cookie(&state).await;
    match ncm::playlist_tracks(&c, id).await {
        Ok(tracks) => {
            if enabled {
                let _ = state.cache.write_json(&key, &tracks);
            }
            Ok(cache::CacheHit {
                data: tracks,
                from_cache: false,
                age_secs: 0,
            })
        }
        Err(e) => {
            if let Some(hit) = state.cache.read_json::<Vec<Track>>(&key) {
                return Ok(hit);
            }
            Err(e)
        }
    }
}

#[tauri::command]
async fn song_url(state: State<'_, AppState>, id: i64) -> R<Option<String>> {
    let c = cookie(&state).await;
    let level = state.settings.lock().await.quality.clone();
    ncm::song_url(&c, id, &level).await
}

/// 播放地址：优先本地音频缓存，否则返回 URL 并后台下载。
/// `refresh=true` 时丢弃该曲缓存并重新取直链（卡顿/损坏恢复用）。
#[tauri::command]
async fn song_play_src(
    state: State<'_, AppState>,
    id: i64,
    refresh: Option<bool>,
) -> R<Option<cache::PlaySrc>> {
    let refresh = refresh.unwrap_or(false);
    let (enabled, quality, max_mb) = {
        let s = state.settings.lock().await;
        (s.cache_enabled, s.quality.clone(), s.audio_cache_max_mb)
    };
    let max_bytes = max_mb.saturating_mul(1024 * 1024);

    if enabled {
        if refresh {
            state.cache.invalidate_audio(id, &quality);
        } else if let Some(path) = state.cache.audio_cached_path(id, &quality) {
            return Ok(Some(cache::PlaySrc {
                kind: "file".into(),
                value: path.to_string_lossy().into(),
                from_cache: true,
            }));
        }
    }

    let c = cookie(&state).await;
    let url = ncm::song_url(&c, id, &quality).await?;
    let Some(url) = url else {
        return Ok(None);
    };

    if enabled {
        let cache = state.cache.clone();
        let q = quality.clone();
        let u = url.clone();
        tauri::async_runtime::spawn(async move {
            let _ = cache.download_audio(id, &q, &u, max_bytes).await;
        });
    }

    Ok(Some(cache::PlaySrc {
        kind: "url".into(),
        value: url,
        from_cache: false,
    }))
}

#[tauri::command]
fn cache_stats(state: State<'_, AppState>) -> R<cache::CacheStats> {
    Ok(state.cache.stats())
}

#[tauri::command]
fn cache_clear(state: State<'_, AppState>) -> R<cache::CacheStats> {
    state.cache.clear()?;
    Ok(state.cache.stats())
}


#[tauri::command]
fn get_playback_session(state: State<'_, AppState>) -> R<PlaybackSession> {
    Ok(state.store.load_playback())
}

#[tauri::command]
fn save_playback_session(state: State<'_, AppState>, session: PlaybackSession) -> R<()> {
    state.store.save_playback(&session)
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> R<Settings> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
async fn save_settings(state: State<'_, AppState>, settings: Settings) -> R<()> {
    if !QUALITY_LEVELS.contains(&settings.quality.as_str()) {
        return Err(format!("未知音质: {}", settings.quality));
    }
    state.store.save_settings(&settings)?;
    *state.settings.lock().await = settings;
    Ok(())
}

#[tauri::command]
async fn scan_local(state: State<'_, AppState>) -> R<Vec<Track>> {
    let folders = state.settings.lock().await.local_folders.clone();
    tauri::async_runtime::spawn_blocking(move || local::scan(&folders))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn liked_ids(state: State<'_, AppState>) -> R<Vec<i64>> {
    let c = cookie(&state).await;
    let p = ncm::profile(&c).await?.ok_or("请先登录")?;
    ncm::liked_ids(&c, p.user_id).await
}

#[tauri::command]
async fn like_track(state: State<'_, AppState>, id: i64, like: bool) -> R<()> {
    ncm::like_track(&cookie(&state).await, id, like).await
}


#[tauri::command]
async fn playlist_delete(state: State<'_, AppState>, id: i64) -> R<()> {
    ncm::playlist_delete(&cookie(&state).await, id).await
}

#[tauri::command]
async fn playlist_subscribe(state: State<'_, AppState>, id: i64, subscribe: bool) -> R<()> {
    ncm::playlist_subscribe(&cookie(&state).await, id, subscribe).await
}

#[tauri::command]
async fn get_lyric(state: State<'_, AppState>, id: i64) -> R<LyricPayload> {
    let enabled = state.settings.lock().await.cache_enabled;
    let key = format!("lyric_{id}");
    if enabled {
        if let Some(hit) = state.cache.read_json::<LyricPayload>(&key) {
            return Ok(hit.data);
        }
    }
    let payload = ncm::lyric(&cookie(&state).await, id).await?;
    if enabled {
        let _ = state.cache.write_json(&key, &payload);
    }
    Ok(payload)
}

/// 本地音频旁同名 .lrc；结果可进磁盘缓存（key 含路径哈希）。
#[tauri::command]
async fn get_local_lyric(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> R<LyricPayload> {
    if path.starts_with("content://") {
        let v = tauri::async_runtime::spawn_blocking(move || {
            mobile::call(&app, "readLrc", serde_json::json!({ "uri": path }))
        })
        .await
        .map_err(|e| e.to_string())??;
        let raw = v.get("lrc").and_then(|x| x.as_str()).unwrap_or("").to_string();
        return Ok(local::lyric_from_raw(raw));
    }
    get_local_lyric_file(state, path).await
}

async fn get_local_lyric_file(state: State<'_, AppState>, path: String) -> R<LyricPayload> {
    let enabled = state.settings.lock().await.cache_enabled;
    let key = format!("lyric_local_{:x}", simple_hash(&path));
    if enabled {
        if let Some(hit) = state.cache.read_json::<LyricPayload>(&key) {
            return Ok(hit.data);
        }
    }
    let payload = local::load_sidecar_lrc(&path)?;
    if enabled && !payload.raw.is_empty() {
        let _ = state.cache.write_json(&key, &payload);
    }
    Ok(payload)
}

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}


#[tauri::command]
fn local_playlists_list(state: State<'_, AppState>) -> R<Vec<local_playlists::LocalPlaylist>> {
    Ok(state.local_pls.list())
}

#[tauri::command]
fn local_playlist_create(state: State<'_, AppState>, name: String) -> R<local_playlists::LocalPlaylist> {
    state.local_pls.create(name)
}

#[tauri::command]
fn local_playlist_rename(state: State<'_, AppState>, id: String, name: String) -> R<local_playlists::LocalPlaylist> {
    state.local_pls.rename(&id, name)
}

#[tauri::command]
fn local_playlist_delete(state: State<'_, AppState>, id: String) -> R<()> {
    state.local_pls.delete(&id)
}

#[tauri::command]
fn local_playlist_get(state: State<'_, AppState>, id: String) -> R<local_playlists::LocalPlaylist> {
    state.local_pls.get(&id)
}

#[tauri::command]
fn local_playlist_add_track(
    state: State<'_, AppState>,
    id: String,
    track: model::Track,
) -> R<local_playlists::LocalPlaylist> {
    state.local_pls.add_track(&id, track)
}

#[tauri::command]
fn local_playlist_remove_track(
    state: State<'_, AppState>,
    id: String,
    track_id: String,
    index: Option<usize>,
) -> R<local_playlists::LocalPlaylist> {
    state.local_pls.remove_track(&id, &track_id, index)
}

#[tauri::command]
fn local_playlist_move_track(
    state: State<'_, AppState>,
    id: String,
    from: usize,
    to: usize,
) -> R<local_playlists::LocalPlaylist> {
    state.local_pls.move_track(&id, from, to)
}

#[tauri::command]
fn local_playlist_set_tracks(
    state: State<'_, AppState>,
    id: String,
    tracks: Vec<model::Track>,
) -> R<local_playlists::LocalPlaylist> {
    state.local_pls.set_tracks(&id, tracks)
}

#[tauri::command]
fn show_main_window(app: AppHandle) -> R<()> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg(desktop)]
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let play = MenuItem::with_id(app, "play", "播放 / 暂停", true, None::<&str>)?;
    let prev = MenuItem::with_id(app, "prev", "上一首", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "下一首", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 SlimMusic", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &play, &prev, &next, &quit])?;

    #[cfg(target_os = "windows")]
    let icon = win_tray::icon_for(win_tray::taskbar_is_light());
    #[cfg(not(target_os = "windows"))]
    // 菜单栏用透明底黑色音符模板图（不要用带色块的 app icon，否则会变成白圆坨）
    let icon = Image::from_bytes(include_bytes!("../icons/tray-template.png"))
        .expect("tray-template.png");

    let app_handle = app.clone();
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .tooltip("SlimMusic")
        .icon_as_template(true) // macOS：按 alpha 作模板，系统绘成黑/白
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "play" => {
                let _ = app.emit("player-cmd", "toggle");
            }
            "prev" => {
                let _ = app.emit("player-cmd", "prev");
            }
            "next" => {
                let _ = app.emit("player-cmd", "next");
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(w) = app_handle.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
        })
        .build(app)?;
    #[cfg(target_os = "windows")]
    win_tray::watch(app.clone());
    Ok(())
}

/// Windows 托盘随任务栏主题（SystemUsesLightTheme）切换黑/白图标。
#[cfg(target_os = "windows")]
mod win_tray {
    use tauri::{image::Image, AppHandle};
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    pub fn taskbar_is_light() -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
            .and_then(|k| k.get_value::<u32, _>("SystemUsesLightTheme"))
            .map(|v| v != 0)
            .unwrap_or(false) // 缺省按深色任务栏（Win10/11 默认）
    }

    pub fn icon_for(light: bool) -> Image<'static> {
        if light {
            Image::from_bytes(include_bytes!("../icons/tray-dark.png")).expect("tray-dark.png")
        } else {
            Image::from_bytes(include_bytes!("../icons/tray-light.png")).expect("tray-light.png")
        }
    }

    pub fn watch(app: AppHandle) {
        std::thread::spawn(move || {
            let mut last = taskbar_is_light();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3));
                let now = taskbar_is_light();
                if now != last {
                    last = now;
                    if let Some(tray) = app.tray_by_id("main") {
                        let _ = tray.set_icon(Some(icon_for(now)));
                    }
                }
            }
        });
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(mobile::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let _ = std::fs::create_dir_all(&dir);
            let local_pls = local_playlists::LocalPlaylistStore::new(dir.clone());
            // 移动端：网络/音频缓存放系统 cache 目录（空间不足时可被系统回收）
            #[cfg(mobile)]
            let cache_root = app.path().app_cache_dir().unwrap_or_else(|_| dir.clone());
            #[cfg(desktop)]
            let cache_root = dir.clone();
            let cache = cache::NetCache::new(cache_root);
            let store = Store::new(dir);
            let state = AppState {
                cookie: Mutex::new(store.load_cookie()),
                settings: Mutex::new(store.load_settings()),
                local_pls,
                cache,
                store,
            };
            app.manage(state);
            #[cfg(desktop)]
            {
                build_tray(app.handle())?;
                // 托盘仅图标 + 菜单，不设标题（避免挤占右侧状态栏）
                if let Some(tray) = app.handle().tray_by_id("main") {
                    let _ = tray.set_title(None::<&str>);
                }
            }
            Ok(())
        });
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());
    #[cfg(desktop)]
    let builder = builder.on_window_event(|window, event| {
        // 红灯 / 关闭：隐藏窗口，音频继续在后台播；真正退出走托盘「退出」
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window.hide();
        }
    });
    builder
        .invoke_handler(tauri::generate_handler![
            login_qr_key,
            login_qr_check,
            logout,
            get_profile,
            daily_recommend,
            search,
            my_playlists,
            playlist_tracks,
            song_url,
            song_play_src,
            cache_stats,
            cache_clear,
            get_playback_session,
            save_playback_session,
            get_settings,
            save_settings,
            scan_local,
            liked_ids,
            like_track,
            playlist_subscribe,
            playlist_delete,
            get_lyric,
            get_local_lyric,
            local_playlists_list,
            local_playlist_create,
            local_playlist_rename,
            local_playlist_delete,
            local_playlist_get,
            local_playlist_add_track,
            local_playlist_remove_track,
            local_playlist_move_track,
            local_playlist_set_tracks,
            show_main_window,
            quit_app,
            mobile::native_call,
            mobile::native_listen,
            mobile::platform_info
        ])
        .build(tauri::generate_context!())
        .expect("启动 Tauri 失败")
        .run(|app, event| {
            // macOS：关窗隐藏后点 Dock 图标会发 Reopened；显示并聚焦主窗口
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            let _ = (app, event);
        });
}
