//! The commands the window calls. The page is untrusted: every argument is
//! checked here (ids are UUIDs, paths are recent projects or files git lists,
//! links are http(s)), and a permission answer can only name a prompt the CLI
//! is waiting on.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_core::cli::{self, Decision, Session};
use forge_core::events::{self, Envelope, SessionEvent};
use forge_core::settings::{self, Settings};
use forge_core::{git, recent, sessions};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::paths;

pub struct AppState {
    pub project: Mutex<Option<PathBuf>>,
    pub running: Mutex<HashMap<String, Arc<Session>>>,
    events: Mutex<Sender<Out>>,
    cache: sessions::Cache,
}

enum Out {
    Event(Envelope),
    Attach(Channel<Vec<Envelope>>),
}

/// One frame: at most one IPC message to the window per 16 ms.
const FRAME: Duration = Duration::from_millis(16);
const MAX_QUEUED: usize = 50_000;

/// The emitter: batches session events per frame, merges deltas, and keeps
/// them while no window is attached (startup, a reload).
fn emitter(rx: mpsc::Receiver<Out>) {
    let mut channel: Option<Channel<Vec<Envelope>>> = None;
    let mut queue: Vec<Envelope> = vec![];
    loop {
        let first = match rx.recv() {
            Ok(o) => o,
            Err(_) => return,
        };
        let mut batch = vec![];
        let handle = |o: Out, batch: &mut Vec<Envelope>, channel: &mut Option<Channel<Vec<Envelope>>>| match o {
            Out::Event(e) => batch.push(e),
            Out::Attach(c) => *channel = Some(c),
        };
        handle(first, &mut batch, &mut channel);
        let deadline = Instant::now() + FRAME;
        loop {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(o) => handle(o, &mut batch, &mut channel),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        queue.append(&mut batch);
        if queue.is_empty() {
            continue;
        }
        let out = events::coalesce(std::mem::take(&mut queue));
        match channel.as_ref() {
            Some(c) if c.send(out.clone()).is_ok() => {}
            _ => {
                channel = None;
                queue = out;
                if queue.len() > MAX_QUEUED {
                    queue.drain(..queue.len() - MAX_QUEUED);
                }
            }
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new().name("events".into()).spawn(move || emitter(rx)).expect("the event thread starts");
        AppState { project: Mutex::default(), running: Mutex::default(), events: Mutex::new(tx), cache: sessions::Cache::default() }
    }
}

impl AppState {
    pub fn emit(&self, event: Envelope) {
        let _ = self.events.lock().unwrap().send(Out::Event(event));
    }

    pub fn stop_all(&self) {
        let all: Vec<_> = self.running.lock().unwrap().drain().map(|(_, s)| s).collect();
        for s in all {
            s.stop();
        }
    }
}

fn state(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

fn project(app: &AppHandle) -> Result<PathBuf, String> {
    state(app).project.lock().unwrap().clone().ok_or_else(|| "no project is open".into())
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    paths::dirs(app).map(|d| d.data)
}

fn folder_name(p: &std::path::Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
}

fn check_id(id: &str) -> Result<(), String> {
    if cli::valid_session_id(id) { Ok(()) } else { Err("bad session id".into()) }
}

/// The window's event channel: every session's events, batched per frame.
#[tauri::command]
pub fn events_attach(app: AppHandle, channel: Channel<Vec<Envelope>>) {
    let _ = state(&app).events.lock().unwrap().send(Out::Attach(channel));
}

#[derive(Serialize)]
pub struct RecentProject {
    path: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    workspace: Option<String>,
    name: Option<String>,
    recent: Vec<RecentProject>,
    platform: &'static str,
    version: String,
    claude: Option<String>,
    running: Vec<String>,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> Info {
    let workspace = state(&app).project.lock().unwrap().clone();
    let data = data_dir(&app).unwrap_or_default();
    let recent = recent::Recent::load(&data)
        .folders
        .into_iter()
        .filter(|p| p.is_dir())
        .map(|p| RecentProject { path: p.display().to_string(), name: folder_name(&p) })
        .collect();
    let running = state(&app).running.lock().unwrap().iter().filter(|(_, s)| s.is_running()).map(|(id, _)| id.clone()).collect();
    Info {
        name: workspace.as_deref().map(folder_name),
        workspace: workspace.map(|p| p.display().to_string()),
        recent,
        platform: std::env::consts::OS,
        version: app.package_info().version.to_string(),
        claude: cli::find_claude().map(|p| p.display().to_string()),
        running,
    }
}

/// Stop this project's sessions and open another; the window reloads.
fn switch_to(app: &AppHandle, folder: PathBuf) {
    if state(app).project.lock().unwrap().as_deref() == Some(folder.as_path()) {
        return;
    }
    paths::log("info", &format!("switching project to {}", folder.display()));
    state(app).stop_all();
    crate::open_project(app, folder);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval("location.reload()");
    }
}

/// `path`: one of the recent folders. None: ask with the folder picker.
#[tauri::command]
pub fn open_project(app: AppHandle, path: Option<String>) -> Result<(), String> {
    match path {
        Some(path) => {
            let wanted = PathBuf::from(&path);
            let known = recent::Recent::load(&data_dir(&app)?).folders.into_iter().any(|p| p == wanted);
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
pub async fn sessions_list(app: AppHandle) -> Result<Vec<sessions::Summary>, String> {
    let Ok(cwd) = project(&app) else { return Ok(vec![]) };
    let hidden = Settings::load(&data_dir(&app)?).archived;
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || sessions::list(&cwd, &state(&handle).cache, &hidden)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn session_load(app: AppHandle, id: String) -> Result<Vec<SessionEvent>, String> {
    check_id(&id)?;
    let cwd = project(&app)?;
    tauri::async_runtime::spawn_blocking(move || match sessions::file(&cwd, &id) {
        Some(path) => sessions::load(&path).map(|m| events::replay(&m)),
        None => Ok(vec![]),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn session_new() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn running(app: &AppHandle, id: &str) -> Option<Arc<Session>> {
    state(app).running.lock().unwrap().get(id).filter(|s| s.is_running()).cloned()
}

/// Send a message, starting (or resuming) the session's CLI if it is not running.
#[tauri::command]
pub async fn session_send(app: AppHandle, id: String, text: String, model: Option<String>, mode: Option<String>) -> Result<(), String> {
    check_id(&id)?;
    if let Some(s) = running(&app, &id) {
        return s.send(&text);
    }
    let cwd = project(&app)?;
    let binary = cli::find_claude().ok_or("Claude Code is not installed: no `claude` on PATH")?;
    let settings = Settings::load(&data_dir(&app)?);
    let profile = settings.active().cloned();
    let model = model
        .filter(|m| cli::valid_model(m))
        .or_else(|| profile.as_ref().map(|p| p.model.clone()).filter(|m| !m.is_empty()))
        .or(settings.default_model.clone());
    let mode = mode.filter(|m| cli::MODES.contains(&m.as_str())).unwrap_or(settings.default_mode.clone());
    let env = profile.map(|p| p.env(settings::key(&p.id))).unwrap_or_default();
    let handle = app.clone();
    let session = tauri::async_runtime::spawn_blocking(move || {
        let resume = sessions::file(&cwd, &id).is_some();
        let opts = cli::Options {
            binary,
            cwd,
            session_id: id,
            resume,
            model,
            // Always explicit: the CLI's own default can be `auto` (it is when
            // nonessential traffic is off), and the window must not say "Ask" then.
            permission_mode: Some(mode),
            env,
            env_remove: vec![],
        };
        let sink_app = handle.clone();
        Session::start(opts, Arc::new(move |e| state(&sink_app).emit(e)))
    })
    .await
    .map_err(|e| e.to_string())??;
    session.send(&text)?;
    let old = state(&app).running.lock().unwrap().insert(session.id.clone(), session);
    if let Some(old) = old {
        old.stop();
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    kind: String,
    message: Option<String>,
    answers: Option<HashMap<String, String>>,
}

#[tauri::command]
pub fn session_respond(app: AppHandle, id: String, request_id: String, answer: Answer) -> Result<(), String> {
    check_id(&id)?;
    let s = running(&app, &id).ok_or("that session is not running")?;
    let decision = match answer.kind.as_str() {
        "allow" => Decision::Allow,
        "always" => Decision::AllowAlways,
        "deny" => Decision::Deny(answer.message.unwrap_or_default()),
        "answer" => Decision::Answer(answer.answers.ok_or("no answers")?.into_iter().map(|(q, a)| (q, Value::String(a))).collect::<Map<String, Value>>()),
        _ => return Err("unknown answer".into()),
    };
    s.respond(&request_id, decision)
}

#[tauri::command]
pub fn session_interrupt(app: AppHandle, id: String) -> Result<(), String> {
    check_id(&id)?;
    running(&app, &id).map_or(Ok(()), |s| s.interrupt())
}

/// Change a running session's mode or model; a stopped one takes them on its next start.
#[tauri::command]
pub fn session_configure(app: AppHandle, id: String, mode: Option<String>, model: Option<String>) -> Result<(), String> {
    check_id(&id)?;
    let Some(s) = running(&app, &id) else { return Ok(()) };
    if let Some(mode) = mode {
        s.set_mode(&mode)?;
    }
    if let Some(model) = model {
        s.set_model(&model)?;
    }
    Ok(())
}

#[tauri::command]
pub fn session_stop(app: AppHandle, id: String) -> Result<(), String> {
    check_id(&id)?;
    if let Some(s) = state(&app).running.lock().unwrap().remove(&id) {
        s.stop();
    }
    Ok(())
}

#[tauri::command]
pub fn session_rename(app: AppHandle, id: String, title: String) -> Result<(), String> {
    check_id(&id)?;
    let path = sessions::file(&project(&app)?, &id).ok_or("that conversation has no saved file yet")?;
    sessions::rename(&path, &id, &title)
}

#[tauri::command]
pub fn session_archive(app: AppHandle, id: String, archived: bool) -> Result<(), String> {
    check_id(&id)?;
    let data = data_dir(&app)?;
    let mut s = Settings::load(&data);
    s.archived.retain(|a| a != &id);
    if archived {
        s.archived.push(id);
    }
    s.save(&data)
}

#[tauri::command]
pub fn settings_get(app: AppHandle) -> Result<Settings, String> {
    Ok(Settings::load(&data_dir(&app)?).with_key_flags())
}

#[tauri::command]
pub fn settings_save(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    let data = data_dir(&app)?;
    let mut s = settings.validated()?;
    // Archive is changed through session_archive, not overwritten wholesale.
    s.archived = Settings::load(&data).archived;
    s.save(&data)?;
    Ok(s.with_key_flags())
}

/// Store (or, with no key, remove) a profile's key in the keychain.
#[tauri::command]
pub fn profile_key(app: AppHandle, id: String, key: Option<String>) -> Result<(), String> {
    let known = Settings::load(&data_dir(&app)?).profiles.iter().any(|p| p.id == id);
    if !known {
        return Err("save the profile first".into());
    }
    settings::set_key(&id, key.as_deref())
}

#[tauri::command]
pub async fn git_status(app: AppHandle) -> Result<git::Status, String> {
    let Ok(dir) = project(&app) else { return Ok(git::Status { is_repo: false, branch: None, files: vec![] }) };
    tauri::async_runtime::spawn_blocking(move || git::status(&dir)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn git_diff(app: AppHandle, path: String) -> Result<git::FileDiff, String> {
    let dir = project(&app)?;
    if path.len() > 4096 {
        return Err("path too long".into());
    }
    tauri::async_runtime::spawn_blocking(move || git::file_diff(&dir, &path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_discard(app: AppHandle, path: String) -> Result<(), String> {
    let dir = project(&app)?;
    if path.len() > 4096 {
        return Err("path too long".into());
    }
    paths::log("info", &format!("discarding changes to {path}"));
    tauri::async_runtime::spawn_blocking(move || git::discard(&dir, &path)).await.map_err(|e| e.to_string())?
}

/// The title bar's buttons.
#[tauri::command]
pub fn window_action(window: WebviewWindow, action: String) -> Result<bool, String> {
    let result = match action.as_str() {
        "minimize" => window.minimize(),
        "toggleMaximize" => {
            if window.is_maximized().unwrap_or(false) { window.unmaximize() } else { window.maximize() }
        }
        "close" => window.close(),
        "isMaximized" => return Ok(window.is_maximized().unwrap_or(false)),
        _ => return Err("unknown window action".into()),
    };
    result.map(|_| window.is_maximized().unwrap_or(false)).map_err(|e| e.to_string())
}

/// The page painted its first frame: show the window (it starts hidden, so
/// there is no white flash).
#[tauri::command]
pub fn window_ready(window: WebviewWindow) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

/// An OS notification, only while the window is not focused. Capped.
#[tauri::command]
pub fn notify(app: AppHandle, window: WebviewWindow, title: String, body: String) -> Result<(), String> {
    if window.is_focused().unwrap_or(false) {
        return Ok(());
    }
    use tauri_plugin_notification::NotificationExt;
    let cap = |s: String, n: usize| s.chars().take(n).collect::<String>();
    app.notification().builder().title(cap(title, 80)).body(cap(body, 240)).show().map_err(|e| e.to_string())
}

/// Open a web link in the browser: http and https only.
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    let ok = (url.starts_with("https://") || url.starts_with("http://")) && url.len() <= 4096 && !url.chars().any(char::is_whitespace);
    if !ok {
        return Err("only web links open".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Show a project file in the system file manager: only a file inside the project.
#[tauri::command]
pub fn reveal(app: AppHandle, path: String) -> Result<(), String> {
    let root = project(&app)?;
    let p = PathBuf::from(&path);
    let full = if p.is_absolute() { p } else { root.join(p) };
    let full = full.canonicalize().map_err(|_| "no such file")?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if !full.starts_with(&root) {
        return Err("only files in the project".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener().reveal_item_in_dir(full).map_err(|e| e.to_string())
}
