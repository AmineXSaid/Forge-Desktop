//! What the desktop shell asks of the app, beyond Forge's own protocol:
//! the project and its recent siblings, switching projects, the Changes pane's
//! git data, and the window buttons of the custom title bar.
//!
//! The page reaches these through `window.forgeDesktop` (the initialization
//! script in lib.rs). Each one checks its input: a project to switch to must
//! be a recent one (anything else goes through the folder picker), and a file
//! to diff must be one git lists as changed.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::sidecar::Bridge;
use crate::{git, paths, recent};

/// The project this window works in.
#[derive(Default)]
pub struct Project(pub Mutex<Option<PathBuf>>);

fn folder_name(p: &std::path::Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    workspace: Option<String>,
    name: Option<String>,
    recent: Vec<Value>,
    platform: &'static str,
    version: String,
}

fn current(app: &AppHandle) -> Option<PathBuf> {
    app.state::<Project>().0.lock().unwrap().clone()
}

#[tauri::command]
pub fn desktop_info(app: AppHandle) -> Info {
    let workspace = current(&app);
    let data = paths::dirs(&app).map(|d| d.data).unwrap_or_default();
    let recent = recent::Recent::load(&data)
        .folders
        .into_iter()
        .filter(|p| p.is_dir())
        .map(|p| json!({"path": p.display().to_string(), "name": folder_name(&p)}))
        .collect();
    Info {
        name: workspace.as_deref().map(folder_name),
        workspace: workspace.map(|p| p.display().to_string()),
        recent,
        platform: std::env::consts::OS,
        version: app.package_info().version.to_string(),
    }
}

/// Restart the host in another folder and reload the page.
fn switch_to(app: &AppHandle, folder: PathBuf) {
    if current(app).as_deref() == Some(folder.as_path()) {
        return;
    }
    paths::log("info", &format!("switching project to {}", folder.display()));
    app.state::<Bridge>().reset();
    crate::open_project(app, folder);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval("location.reload()");
    }
}

/// `path`: one of the recent folders. None: ask with the folder picker.
#[tauri::command]
pub fn desktop_open_project(app: AppHandle, path: Option<String>) -> Result<(), String> {
    match path {
        Some(path) => {
            let wanted = PathBuf::from(&path);
            let data = paths::dirs(&app)?.data;
            let known = recent::Recent::load(&data).folders.into_iter().any(|p| p == wanted);
            if !known || !wanted.is_dir() {
                return Err("only a recent project opens by path; use Open folder… for another".into());
            }
            switch_to(&app, wanted);
            Ok(())
        }
        None => {
            let handle = app.clone();
            let mut picker = app.dialog().file().set_title("Open a project folder");
            if let Some(window) = app.get_webview_window("main") {
                picker = picker.set_parent(&window);
            }
            picker.pick_folder(move |picked| {
                if let Some(folder) = picked.and_then(|p| p.into_path().ok()) {
                    switch_to(&handle, crate::clean(folder));
                }
            });
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn desktop_git_status(app: AppHandle) -> Result<git::Status, String> {
    let Some(dir) = current(&app) else { return Ok(git::Status { is_repo: false, branch: None, files: vec![] }) };
    tauri::async_runtime::spawn_blocking(move || git::status(&dir)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn desktop_git_diff(app: AppHandle, path: String) -> Result<git::FileDiff, String> {
    let dir = current(&app).ok_or("no project is open")?;
    if path.len() > 4096 {
        return Err("path too long".into());
    }
    tauri::async_runtime::spawn_blocking(move || git::file_diff(&dir, &path)).await.map_err(|e| e.to_string())?
}

/// The title bar's buttons.
#[tauri::command]
pub fn desktop_window(window: WebviewWindow, action: String) -> Result<Value, String> {
    let result = match action.as_str() {
        "minimize" => window.minimize(),
        "toggleMaximize" => {
            if window.is_maximized().unwrap_or(false) { window.unmaximize() } else { window.maximize() }
        }
        "close" => window.close(),
        "isMaximized" => return Ok(json!(window.is_maximized().unwrap_or(false))),
        _ => return Err("unknown window action".into()),
    };
    result.map(|_| Value::Null).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn desktop_git_discard(app: AppHandle, path: String) -> Result<(), String> {
    let dir = current(&app).ok_or("no project is open")?;
    if path.len() > 4096 {
        return Err("path too long".into());
    }
    paths::log("info", &format!("discarding changes to {path}"));
    tauri::async_runtime::spawn_blocking(move || git::discard(&dir, &path)).await.map_err(|e| e.to_string())?
}

/// An OS notification, only while the window is not focused (a focused window
/// already shows what happened). Title and body are capped.
#[tauri::command]
pub fn desktop_notify(app: AppHandle, window: WebviewWindow, title: String, body: String) -> Result<(), String> {
    if window.is_focused().unwrap_or(false) {
        return Ok(());
    }
    use tauri_plugin_notification::NotificationExt;
    let cap = |s: String, n: usize| s.chars().take(n).collect::<String>();
    app.notification()
        .builder()
        .title(cap(title, 80))
        .body(cap(body, 240))
        .show()
        .map_err(|e| e.to_string())
}
