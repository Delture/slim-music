//! 刘海左侧菜单栏歌词条：透明置顶小窗，不是 NSStatusItem。
//! 托盘图标仍在右侧，只负责控制；歌词画在应用菜单之后、刘海之前。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

const WIN: &str = "menubar-lyric";
const H_LOGICAL: f64 = 22.0;
const MAX_W_LOGICAL: f64 = 340.0;
const MIN_W_LOGICAL: f64 = 120.0;
/// 给左侧 Apple/应用菜单留空（无 Accessibility 时的保守估计）
const LEFT_MENU_PAD: f64 = 168.0;

static MAIN_VISIBLE: AtomicBool = AtomicBool::new(true);
static LAST_TEXT: Mutex<String> = Mutex::new(String::new());

pub fn set_main_visible(app: &AppHandle, visible: bool) {
    MAIN_VISIBLE.store(visible, Ordering::Relaxed);
    let _ = app.emit("main-window-visible", visible);
    refresh(app);
}

pub fn main_is_visible() -> bool {
    MAIN_VISIBLE.load(Ordering::Relaxed)
}

/// 前端推送当前歌词行；空字符串表示清空。
pub fn set_text(app: &AppHandle, text: String) {
    {
        let mut g = LAST_TEXT.lock().expect("lyric lock");
        *g = text.trim().to_string();
    }
    refresh(app);
}

fn current_text() -> String {
    LAST_TEXT.lock().expect("lyric lock").clone()
}

fn refresh(app: &AppHandle) {
    let text = current_text();
    let show = !main_is_visible() && !text.is_empty();
    if show {
        if let Ok(w) = ensure_window(app) {
            reposition(&w, app);
            elevate_macos(&w);
            let _ = w.emit("menubar-lyric-text", text.clone());
            let _ = w.show();
        }
    } else if let Some(w) = app.get_webview_window(WIN) {
        let _ = w.emit("menubar-lyric-text", String::new());
        let _ = w.hide();
    }
}

fn ensure_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(WIN) {
        return Ok(w);
    }
    let url = if cfg!(debug_assertions) {
        WebviewUrl::External(
            "http://localhost:1420/lyric-bar.html"
                .parse()
                .map_err(|e| format!("{e}"))?,
        )
    } else {
        WebviewUrl::App("lyric-bar.html".into())
    };
    let w = WebviewWindowBuilder::new(app, WIN, url)
        .title("SlimMusic Lyric")
        .inner_size(MAX_W_LOGICAL, H_LOGICAL)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .shadow(false)
        .build()
        .map_err(|e| e.to_string())?;
    elevate_macos(&w);
    Ok(w)
}

fn reposition(win: &tauri::WebviewWindow, app: &AppHandle) {
    let Some(monitor) = (|| {
        win.current_monitor()
            .ok()
            .flatten()
            .or_else(|| app.primary_monitor().ok().flatten())
    })() else {
        return;
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size();
    let pos = monitor.position();
    let sw = screen.width as f64 / scale;

    // 估算刘海宽度：14"/16" MacBook 逻辑宽度通常 >=1470 / 1728
    let notch_w = if sw >= 1700.0 {
        220.0
    } else if sw >= 1400.0 {
        180.0
    } else if sw >= 1200.0 {
        160.0
    } else {
        0.0
    };

    // 歌词条右对齐贴在刘海左侧（应用菜单之后、刘海之前）
    let notch_left = sw * 0.5 - notch_w * 0.5;
    let right = if notch_w > 0.0 {
        notch_left - 6.0
    } else {
        (sw * 0.45).min(sw - 280.0)
    };
    let left_min = LEFT_MENU_PAD;
    let avail = (right - left_min).max(MIN_W_LOGICAL);
    let lw = avail.min(MAX_W_LOGICAL).max(MIN_W_LOGICAL);
    let lx = (right - lw).max(left_min);
    let ly = 1.0;

    let px = pos.x + (lx * scale).round() as i32;
    let py = pos.y + (ly * scale).round() as i32;
    let pw = (lw * scale).round() as u32;
    let ph = (H_LOGICAL * scale).round() as u32;
    let _ = win.set_size(PhysicalSize::new(pw, ph));
    let _ = win.set_position(PhysicalPosition::new(px, py));
}

#[cfg(target_os = "macos")]
fn elevate_macos(win: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let Ok(ptr) = win.ns_window() else {
        return;
    };
    if ptr.is_null() {
        return;
    }
    unsafe {
        let window = &*(ptr as *const AnyObject);
        // NSMainMenuWindowLevel = 24；再抬一级，看起来像嵌在菜单栏里
        let level: i64 = 25;
        let _: () = msg_send![window, setLevel: level];
        let _: () = msg_send![window, setIgnoresMouseEvents: true];
        // CanJoinAllSpaces|Stationary|FullScreenAuxiliary
        let behavior: u64 = 1 << 0 | 1 << 4 | 1 << 8;
        let _: () = msg_send![window, setCollectionBehavior: behavior];
        let _: () = msg_send![window, setHidesOnDeactivate: false];
    }
}

#[cfg(not(target_os = "macos"))]
fn elevate_macos(_win: &tauri::WebviewWindow) {}

pub fn on_app_setup(app: &AppHandle) {
    MAIN_VISIBLE.store(true, Ordering::Relaxed);
    let _ = ensure_window(app);
}
