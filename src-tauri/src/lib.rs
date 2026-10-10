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
            desktop::window_ready,
            desktop::notify,
            desktop::open_url,
            desktop::reveal,
            desktop::cc_settings_get,
            desktop::cc_settings_set,
            desktop::hook_add,
            desktop::hook_remove,
            desktop::memory_read,
            desktop::memory_write,
            desktop::extensions_list,
            desktop::open_file,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            forge_core::log::init(&log_dir_for(&handle));
            paths::log("info", &format!("Forge Desktop {} starting", handle.package_info().version));
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Forge")
                .inner_size(1180.0, 800.0)
                .min_inner_size(760.0, 520.0)
                // The page draws its own title bar.
                .decorations(false)
                // Shown by the page once it has painted (window_ready).
                .visible(false)
                .background_color(tauri::window::Color(0xf6, 0xf5, 0xf4, 0xff))
                .build()?;
            // If the page never says it is ready (a broken page, a slow disk),
            // show the window anyway rather than leave an invisible app.
            let late = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if !late.is_visible().unwrap_or(true) {
                    paths::log("warn", "the page did not report ready; showing the window");
                    let _ = late.show();
                }
            });
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

#[cfg(test)]
mod contract {
    //! The UI's TypeScript contract (`ui/src/lib/ipc.ts`) must name every
    //! command this app registers and every kind of session event it sends.

    fn read(rel: &str) -> String {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).unwrap()
    }

    #[test]
    fn every_command_is_typed_in_the_ui() {
        let ipc = read("../ui/src/lib/ipc.ts");
        let lib = read("src/lib.rs");
        let handler = &lib[lib.find("generate_handler![").unwrap()..];
        let handler = &handler[..handler.find("])").unwrap()];
        let names: Vec<&str> = handler.split("desktop::").skip(1).map(|s| s.split(',').next().unwrap().trim()).collect();
        assert!(names.len() > 20, "found {names:?}");
        for n in names {
            assert!(ipc.contains(&format!("  {n}: [")), "ui/src/lib/ipc.ts has no type for the command `{n}`");
        }
    }

    #[test]
    fn every_session_event_kind_is_typed_in_the_ui() {
        let ipc = read("../ui/src/lib/ipc.ts");
        let events = read("../crates/forge-core/src/events.rs");
        let body = &events[events.find("pub enum SessionEvent {").unwrap()..];
        let body = &body[..body.find("\n}").unwrap()];
        let mut kinds = vec![];
        for line in body.lines().skip(1) {
            let t = line.trim_start();
            if line.starts_with("    ") && !line.starts_with("     ") && t.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                let name: String = t.chars().take_while(|c| c.is_alphanumeric()).collect();
                let mut camel = name.clone();
                camel[..1].make_ascii_lowercase();
                kinds.push(camel);
            }
        }
        assert!(kinds.len() >= 15, "found {kinds:?}");
        for k in kinds {
            assert!(ipc.contains(&format!("kind: '{k}'")), "ui/src/lib/ipc.ts has no SessionEvent of kind `{k}`");
        }
    }
}
