//! Android 原生桥：前台播放服务（Media3）与 MediaStore 本地音乐。
//! 前端经 app 命令 `native_call` / `native_listen` 访问，避免为内联插件单独配置 ACL。
use serde_json::Value;
use tauri::{ipc::Channel, AppHandle, Runtime};

#[cfg(target_os = "android")]
pub struct NativeHandle<R: Runtime>(pub tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("slimnative")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let h = _api.register_android_plugin("dev.slimmusic.app", "SlimNativePlugin")?;
                _app.manage(NativeHandle(h));
            }
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
pub fn call<R: Runtime>(app: &AppHandle<R>, method: &str, args: Value) -> Result<Value, String> {
    use tauri::Manager;
    let h = app
        .try_state::<NativeHandle<R>>()
        .ok_or("native plugin unavailable")?;
    h.0.run_mobile_plugin::<Value>(method, args)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
pub fn call<R: Runtime>(_app: &AppHandle<R>, method: &str, _args: Value) -> Result<Value, String> {
    Err(format!("native `{method}` is Android-only"))
}

#[tauri::command]
pub async fn native_call(app: AppHandle, method: String, args: Option<Value>) -> Result<Value, String> {
    let args = args.unwrap_or(Value::Object(Default::default()));
    tauri::async_runtime::spawn_blocking(move || call(&app, &method, args))
        .await
        .map_err(|e| e.to_string())?
}

/// 注册原生播放器事件回调（状态 / 进度 / 结束 / 通知栏上一首下一首）。
#[tauri::command]
pub async fn native_listen(app: AppHandle, on_event: Channel<Value>) -> Result<(), String> {
    let args = serde_json::json!({ "handler": on_event });
    tauri::async_runtime::spawn_blocking(move || call(&app, "setHandler", args).map(|_| ()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn platform_info() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else {
        "desktop"
    }
}
