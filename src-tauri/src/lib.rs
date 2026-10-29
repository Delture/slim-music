//! Tauri 入口：所有前端调用都经由这里的 #[tauri::command]。
mod local;
mod local_playlists;
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
async fn my_playlists(state: State<'_, AppState>) -> R<Vec<Playlist>> {
    let c = cookie(&state).await;
    let p = ncm::profile(&c).await?.ok_or("请先登录")?;
    ncm::user_playlists(&c, p.user_id).await
}

#[tauri::command]
async fn playlist_tracks(state: State<'_, AppState>, id: i64) -> R<Vec<Track>> {
    ncm::playlist_tracks(&cookie(&state).await, id).await
}

#[tauri::command]
async fn song_url(state: State<'_, AppState>, id: i64) -> R<Option<String>> {
    let c = cookie(&state).await;
    let level = state.settings.lock().await.quality.clone();
    ncm::song_url(&c, id, &level).await
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

/// 更新菜单栏 / 托盘标题（macOS 上显示在刘海旁；其他平台多为 tooltip）
#[tauri::command]
fn set_tray_title(app: AppHandle, text: String) -> R<()> {
    if let Some(tray) = app.tray_by_id("main") {
        // macOS：标题会出现在图标旁；限制长度以免撑满菜单栏
        let short = if text.chars().count() > 24 {
            format!("{}…", text.chars().take(23).collect::<String>())
        } else {
            text.clone()
        };
        let _ = tray.set_title(Some(short));
        let _ = tray.set_tooltip(Some(text));
    }
    Ok(())
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

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let play = MenuItem::with_id(app, "play", "播放 / 暂停", true, None::<&str>)?;
    let prev = MenuItem::with_id(app, "prev", "上一首", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "下一首", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 SlimMusic", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &play, &prev, &next, &quit])?;

    let icon = app.default_window_icon().cloned().unwrap_or_else(|| {
        Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("tray icon")
    });

    let app_handle = app.clone();
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .tooltip("SlimMusic")
        .icon_as_template(true) // macOS 菜单栏：按模板着色
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
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let local_pls = local_playlists::LocalPlaylistStore::new(dir.clone());
            let store = Store::new(dir);
            let state = AppState {
                cookie: Mutex::new(store.load_cookie()),
                settings: Mutex::new(store.load_settings()),
                local_pls,
                store,
            };
            app.manage(state);
            build_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // 红灯 / 关闭：隐藏窗口，音频继续在后台播；真正退出走托盘「退出」
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
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
            get_settings,
            save_settings,
            scan_local,
            liked_ids,
            like_track,
            playlist_subscribe,
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
            show_main_window,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 失败");
}
