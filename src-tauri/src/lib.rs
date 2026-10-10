//! Forge Desktop: a desktop app for Claude Code.
//!
//! 1. The project: a folder given on the command line, else the last one used
//!    (`recent.json`), else a folder picker.
//! 2. The window is this app's own page (`web/`). It talks to Rust through the
//!    commands in `desktop.rs` and one event channel.
//! 3. Each chat session is a `claude` CLI process in the project folder
//!    (`cli.rs`), started when the session gets its first message.

mod desktop;
mod paths;

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

use desktop::AppState;
use forge_core::recent;

/// The project folder from the command line: the first argument that is a directory.
fn folder_from_args() -> Option<PathBuf> {
    std::env::args_os().skip(1).map(PathBuf::from).find(|p| p.is_dir()).and_then(|p| p.canonicalize().ok()).map(clean)
}

/// `canonicalize` on Windows gives `\\?\C:\...`; the CLI wants `C:\...`.
pub(crate) fn clean(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

pub(crate) fn open_project(app: &AppHandle, folder: PathBuf) {
    if let Ok(dirs) = paths::dirs(app) {
        let mut recent = recent::Recent::load(&dirs.data);
        recent.push(&folder);
        if let Err(e) = recent.save(&dirs.data) {
            paths::log("warn", &format!("could not save recent.json: {e}"));
        }
    }
    paths::log("info", &format!("project {}", folder.display()));
    if let Some(window) = app.get_webview_window("main") {
        let name = folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| folder.display().to_string());
        let _ = window.set_title(&format!("{name} — Forge"));
    }
    *app.state::<AppState>().project.lock().unwrap() = Some(folder);
}

fn choose_project(app: &AppHandle) {
    if let Some(folder) = folder_from_args() {
        return open_project(app, folder);
    }
    let data = paths::dirs(app).map(|d| d.data).unwrap_or_default();
    if let Some(folder) = recent::Recent::load(&data).last().cloned() {
        return open_project(app, folder);
    }
    let handle = app.clone();
    let mut picker = app.dialog().file().set_title("Open a project folder");
    if let Some(window) = app.get_webview_window("main") {
        picker = picker.set_parent(&window);
    }
    picker.pick_folder(move |picked| match picked.and_then(|p| p.into_path().ok()) {
        Some(folder) => {
            open_project(&handle, clean(folder));
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.eval("location.reload()");
            }
        }
        None => {
            paths::log("info", "no folder chosen; exiting");
            handle.exit(0);
        }
    });
}

fn log_dir_for(app: &AppHandle) -> PathBuf {
    paths::dirs(app).map(|d| d.logs).unwrap_or_else(|_| Path::new(".").to_path_buf())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            desktop::events_attach,
            desktop::app_info,
            desktop::open_project,
            desktop::sessions_list,
            desktop::session_load,
            desktop::session_new,
            desktop::session_send,
            desktop::session_respond,
            desktop::session_interrupt,
            desktop::session_configure,
            desktop::session_stop,
            desktop::session_rename,
            desktop::session_archive,
            desktop::settings_get,
            desktop::settings_save,
            desktop::profile_key,
            desktop::git_status,
            desktop::git_diff,
            desktop::git_discard,
            desktop::window_action,
            desktop::notify,
            desktop::open_url,
            desktop::reveal,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            forge_core::log::init(&log_dir_for(&handle));
            paths::log("info", &format!("Forge Desktop {} starting", handle.package_info().version));
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Forge")
                .inner_size(1180.0, 800.0)
                .min_inner_size(760.0, 520.0)
                // The page draws its own title bar.
                .decorations(false)
                .build()?;
            choose_project(&handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Forge Desktop");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            paths::log("info", "exiting");
            handle.state::<AppState>().stop_all();
        }
    });
}
