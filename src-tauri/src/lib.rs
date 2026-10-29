//! Tauri 入口：所有前端调用都经由这里的 #[tauri::command]。
mod cache;
mod local;
mod local_playlists;
mod menubar_lyric;
mod model;
mod ncm;
mod settings;

use model::{LyricPayload, Playlist, Profile, Track};
use settings::{Settings, Store, QUALITY_LEVELS};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
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

/// 播放地址：优先本地音频缓存，否则返回 URL 并后台下载
#[tauri::command]
async fn song_play_src(state: State<'_, AppState>, id: i64) -> R<Option<cache::PlaySrc>> {
    let (enabled, quality, max_mb) = {
        let s = state.settings.lock().await;
        (s.cache_enabled, s.quality.clone(), s.audio_cache_max_mb)
    };
    let max_bytes = max_mb.saturating_mul(1024 * 1024);

    if enabled {
        if let Some(path) = state.cache.audio_cached_path(id, &quality) {
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
    ncm::lyric(&cookie(&state).await, id).await
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

/// 刘海左侧菜单栏歌词（透明置顶窗，非右侧 NSStatusItem 标题）
#[tauri::command]
fn set_menubar_lyric(app: AppHandle, text: String) -> R<()> {
    menubar_lyric::set_text(&app, text);
    Ok(())
}

/// 兼容旧前端：清空右侧托盘标题（歌词不再走托盘）
#[tauri::command]
fn set_tray_title(app: AppHandle, _text: String) -> R<()> {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_title(None::<&str>);
        let _ = tray.set_tooltip(Some("SlimMusic"));
    }
    Ok(())
}

#[tauri::command]
fn show_main_window(app: AppHandle) -> R<()> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        menubar_lyric::set_main_visible(&app, true);
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let play = MenuItem::with_id(app, "play", "播放 / 暂停", true, None::<&str>)?;
    let prev = MenuItem::with_id(app, "prev", "上一首", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "下一首", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 SlimMusic", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &play, &prev, &next, &quit])?;

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
                    menubar_lyric::set_main_visible(app, true);
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
                        menubar_lyric::set_main_visible(&app_handle, false);
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();
                        menubar_lyric::set_main_visible(&app_handle, true);
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let local_pls = local_playlists::LocalPlaylistStore::new(dir.clone());
            let cache = cache::NetCache::new(dir.clone());
            let store = Store::new(dir);
            let state = AppState {
                cookie: Mutex::new(store.load_cookie()),
                settings: Mutex::new(store.load_settings()),
                local_pls,
                cache,
                store,
            };
            app.manage(state);
            build_tray(app.handle())?;
            menubar_lyric::on_app_setup(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // 红灯 / 关闭：隐藏窗口，音频继续在后台播；真正退出走托盘「退出」
            // 仅主窗口参与「桌面窗可见 → 隐藏刘海左歌词」逻辑
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                    menubar_lyric::set_main_visible(window.app_handle(), false);
                }
                WindowEvent::Focused(true) => {
                    // 被唤起时视为可见
                    if window.is_visible().unwrap_or(false) {
                        menubar_lyric::set_main_visible(window.app_handle(), true);
                    }
                }
                _ => {}
            }
        })
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
            get_settings,
            save_settings,
            scan_local,
            liked_ids,
            like_track,
            playlist_subscribe,
            playlist_delete,
            get_lyric,
            local_playlists_list,
            local_playlist_create,
            local_playlist_rename,
            local_playlist_delete,
            local_playlist_get,
            local_playlist_add_track,
            local_playlist_remove_track,
            local_playlist_move_track,
            local_playlist_set_tracks,
            set_tray_title,
            set_menubar_lyric,
            show_main_window,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 失败");
}
