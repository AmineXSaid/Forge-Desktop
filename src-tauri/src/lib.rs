//! Forge Desktop: one window, one project folder, one Forge host (plan §4.5 M1).
//!
//! 1. The project: a folder given on the command line, else the last one used
//!    (`recent.json`), else a folder picker.
//! 2. The window loads Forge's own chat page (staged `index.html`). An
//!    initialization script gives it the `acquireVsCodeApi()` it expects.
//! 3. The host is Forge's extension host in Node (`forge-host.cjs`), joined
//!    to the page through `forge_send` / `forge_attach` and stdio.

mod desktop;
mod git;
mod host_calls;
mod ipc;
mod paths;
mod recent;
mod sidecar;

use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use sidecar::Bridge;

/// What VS Code gives a webview, on Tauri: `acquireVsCodeApi()` whose
/// `postMessage` reaches the host, and host messages as `message` events.
/// The channel attaches on first use, after Tauri's own globals exist.
const INIT_SCRIPT: &str = r#"
(() => {
  let state;
  let attached = false;
  const core = () => window.__TAURI__ && window.__TAURI__.core;
  const attach = () => {
    if (attached || !core()) return;
    attached = true;
    const channel = new (core().Channel)();
    channel.onmessage = (data) => window.dispatchEvent(new MessageEvent('message', { data }));
    core().invoke('forge_attach', { channel }).catch((e) => console.error('[forge] attach failed', e));
  };
  const api = Object.freeze({
    postMessage(message) {
      attach();
      core().invoke('forge_send', { message }).catch((e) => console.error('[forge] a message was refused:', e));
    },
    getState: () => state,
    setState: (next) => (state = next),
  });
  let acquired = false;
  window.acquireVsCodeApi = () => {
    if (acquired) throw new Error('An instance of the VS Code API has already been acquired');
    acquired = true;
    attach();
    return api;
  };
  document.addEventListener('DOMContentLoaded', attach);

  // The desktop shell's own calls (src/desktop.rs); Forge's page reads them
  // through its DesktopHost interface (src/webview/src/desktop/desktopHost.ts).
  const call = (cmd, args) => core().invoke(cmd, args);
  window.forgeDesktop = Object.freeze({
    info: () => call('desktop_info'),
    openProject: (path) => call('desktop_open_project', { path: path ?? null }),
    gitStatus: () => call('desktop_git_status'),
    gitDiff: (path) => call('desktop_git_diff', { path }),
    windowAction: (action) => call('desktop_window', { action }),
  });
})();
"#;

/// This build's page and assets (`scripts/stage.mjs`). In the page URL, so the
/// webview's cache never serves a previous build's page.
const BUILD_ID: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../stage/web/build-id"));

#[tauri::command]
fn forge_send(bridge: tauri::State<'_, Bridge>, message: Value) -> Result<(), String> {
    let message = ipc::check_envelope(message).inspect_err(|e| paths::log("warn", &format!("refused a page message: {e}")))?;
    bridge.to_host(message)
}

#[tauri::command]
fn forge_attach(bridge: tauri::State<'_, Bridge>, channel: Channel<Value>) {
    bridge.attach(channel);
}

/// The project folder from the command line: the first argument that is a directory.
fn folder_from_args() -> Option<PathBuf> {
    std::env::args_os().skip(1).map(PathBuf::from).find(|p| p.is_dir()).and_then(|p| p.canonicalize().ok()).map(clean)
}

/// `canonicalize` on Windows gives `\\?\C:\...`; the CLI and Forge want `C:\...`.
pub(crate) fn clean(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

fn fatal(app: &AppHandle, message: String) {
    paths::log("error", &message);
    let handle = app.clone();
    app.dialog()
        .message(message)
        .title("Forge")
        .kind(MessageDialogKind::Error)
        .show(move |_| handle.exit(1));
}

pub(crate) fn open_project(app: &AppHandle, folder: PathBuf) {
    let dirs = match paths::dirs(app) {
        Ok(d) => d,
        Err(e) => return fatal(app, format!("Forge could not find its folders: {e}")),
    };
    let mut recent = recent::Recent::load(&dirs.data);
    recent.push(&folder);
    if let Err(e) = recent.save(&dirs.data) {
        paths::log("warn", &format!("could not save recent.json: {e}"));
    }
    *app.state::<desktop::Project>().0.lock().unwrap() = Some(folder.clone());
    if let Some(window) = app.get_webview_window("main") {
        let name = folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| folder.display().to_string());
        let _ = window.set_title(&format!("{name} — Forge"));
    }
    let Some(node) = sidecar::find_node() else {
        return fatal(app, "Forge needs Node.js 22 or later, and found no `node` on PATH.\n\nInstall it from https://nodejs.org, or set FORGE_NODE to node.exe, then start Forge again.".into());
    };
    let launch = sidecar::Launch {
        node: &node,
        resources: &dirs.resources,
        workspace: &folder,
        data_dir: &dirs.data,
        log_dir: &dirs.logs,
        version: app.package_info().version.to_string(),
    };
    if let Err(e) = sidecar::start(app, launch) {
        fatal(app, format!("Forge could not start its host: {e}"));
    }
}

/// The host stopped. Unless the app is closing, say so and where the log is.
pub(crate) fn host_exited(app: &AppHandle) {
    if app.get_webview_window("main").is_none() {
        return;
    }
    let log = paths::dirs(app).map(|d| d.logs.join("forge-host.log").display().to_string()).unwrap_or_default();
    app.dialog()
        .message(format!("Forge's host stopped. The chat cannot continue in this window.\n\nLog: {log}"))
        .title("Forge")
        .kind(MessageDialogKind::Error)
        .show(|_| {});
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
        Some(folder) => open_project(&handle, clean(folder)),
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
        .manage(Bridge::default())
        .manage(desktop::Project::default())
        .invoke_handler(tauri::generate_handler![
            forge_send,
            forge_attach,
            desktop::desktop_info,
            desktop::desktop_open_project,
            desktop::desktop_git_status,
            desktop::desktop_git_diff,
            desktop::desktop_window,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            paths::init_log(&log_dir_for(&handle));
            paths::log("info", &format!("Forge Desktop {} starting", handle.package_info().version));
            let page = format!("index.html?build={}", BUILD_ID.trim());
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(page.into()))
                .title("Forge")
                .inner_size(1100.0, 780.0)
                .min_inner_size(720.0, 480.0)
                // The shell draws its own title bar (desktop/TitleBar.vue).
                .decorations(false)
                .theme(Some(tauri::Theme::Dark))
                .initialization_script(INIT_SCRIPT)
                .build()?;
            // A side-bar view is "hidden" when it is not on screen, not when it
            // lacks focus: here that is a minimised window.
            let visibility = handle.clone();
            let watched = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::Resized(_) = event {
                    let visible = !watched.is_minimized().unwrap_or(false);
                    visibility.state::<Bridge>().frame_to_host(serde_json::json!({"$forge": "visibility", "visible": visible}));
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
            handle.state::<Bridge>().stop();
        }
    });
}
