//! Tauri 入口：所有前端调用都经由这里的 #[tauri::command]。
mod local;
mod model;
mod ncm;
mod settings;

use model::{Playlist, Profile, Track};
use settings::{Settings, Store, QUALITY_LEVELS};
use tauri::{Manager, State};
use tokio::sync::Mutex;

struct AppState {
    store: Store,
    cookie: Mutex<Option<String>>,
    settings: Mutex<Settings>,
}

type R<T> = Result<T, String>;

#[derive(serde::Serialize)]
struct QrKeyOut {
    key: String,
    svg: String,
}

#[tauri::command]
async fn login_qr_key() -> R<QrKeyOut> {
    let k = ncm::qr_key().await?;
    Ok(QrKeyOut { key: k.key, svg: k.svg })
}

/// 轮询扫码状态；803 时保存 cookie
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
    let c = state.cookie.lock().await.clone();
    ncm::profile(&c).await
}

#[tauri::command]
async fn daily_recommend(state: State<'_, AppState>) -> R<Vec<Track>> {
    let c = state.cookie.lock().await.clone();
    ncm::daily(&c).await
}

#[tauri::command]
async fn search(state: State<'_, AppState>, keywords: String, offset: u32) -> R<Vec<Track>> {
    let c = state.cookie.lock().await.clone();
    ncm::search(&c, &keywords, offset).await
}

#[tauri::command]
async fn my_playlists(state: State<'_, AppState>) -> R<Vec<Playlist>> {
    let c = state.cookie.lock().await.clone();
    let p = ncm::profile(&c).await?.ok_or("请先登录")?;
    ncm::user_playlists(&c, p.user_id).await
}

#[tauri::command]
async fn playlist_tracks(state: State<'_, AppState>, id: i64) -> R<Vec<Track>> {
    let c = state.cookie.lock().await.clone();
    ncm::playlist_tracks(&c, id).await
}

/// 按设置里的音质取播放地址；无版权/无权限时返回 None
#[tauri::command]
async fn song_url(state: State<'_, AppState>, id: i64) -> R<Option<String>> {
    let c = state.cookie.lock().await.clone();
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

/// 扫描设置中的本地文件夹（在阻塞线程中执行）
#[tauri::command]
async fn scan_local(state: State<'_, AppState>) -> R<Vec<Track>> {
    let folders = state.settings.lock().await.local_folders.clone();
    tauri::async_runtime::spawn_blocking(move || local::scan(&folders))
        .await
        .map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let store = Store::new(dir);
            let state = AppState {
                cookie: Mutex::new(store.load_cookie()),
                settings: Mutex::new(store.load_settings()),
                store,
            };
            app.manage(state);
            Ok(())
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
            scan_local
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 失败");
}
