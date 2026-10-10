//! The commands the window calls. The page is untrusted: every argument is
//! checked here (ids are UUIDs, paths are recent projects or files git lists,
//! links are http(s)), and a permission answer can only name a prompt the CLI
//! is waiting on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use forge_core::cli::{self, Decision, Session, Thinking};
use forge_core::events::{self, Envelope, SessionEvent};
use forge_core::meta::Meta;
use forge_core::session_manager::{self, Limits};
use forge_core::settings::{self, Settings};
use forge_core::catalog::{self, ModelCache};
use forge_core::gateway::Gateway;
use forge_core::{cc_settings, git, recent, sessions, watch};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::paths;

pub struct AppState {
    pub project: Mutex<Option<PathBuf>>,
    /// The live CLI processes (prewarm, reaping, the cap).
    pub live: Arc<session_manager::Manager>,
    events: Mutex<Sender<Out>>,
    /// Set once the data folder is known (`init`).
    index: OnceLock<(sessions::Index, PathBuf)>,
    meta: OnceLock<Meta>,
    /// The session the window shows: its turns don't mark it unread.
    focused: Mutex<Option<String>>,
    watch: Mutex<Option<watch::Watch>>,
    /// The loopback gateway sessions routed to a provider go through.
    gateway: OnceLock<Gateway>,
    /// Keys read from the keychain, by profile id (cleared when one changes).
    keys: Arc<Mutex<HashMap<String, String>>>,
    /// The provider route each live session started with ("" = Claude account).
    routes: Mutex<HashMap<String, String>>,
    models: Mutex<ModelCache>,
    http: forge_core::reqwest::Client,
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
        let live = Arc::new(session_manager::Manager::new(Limits::default()));
        live.spawn_reaper();
        AppState {
            project: Mutex::default(),
            live,
            events: Mutex::new(tx),
            index: OnceLock::new(),
            meta: OnceLock::new(),
            focused: Mutex::default(),
            watch: Mutex::default(),
            gateway: OnceLock::new(),
            keys: Arc::default(),
            routes: Mutex::default(),
            models: Mutex::default(),
            http: forge_core::reqwest::Client::new(),
        }
    }
}

impl AppState {
    pub fn emit(&self, event: Envelope) {
        let _ = self.events.lock().unwrap().send(Out::Event(event));
    }

    pub fn stop_all(&self) {
        self.live.stop_all();
        self.save_index();
    }

    /// Load what is kept in the data folder: the session index and marks.
    pub fn init(&self, data: &Path) {
        let file = data.join("session-index.json");
        let _ = self.index.set((sessions::Index::load(&file), file));
        let _ = self.meta.set(Meta::load(data));
        *self.models.lock().unwrap() = ModelCache::load(data);
        // The gateway reads Settings on each request, so a provider changed in
        // Settings applies to the next request without a restart.
        let (dir, keys) = (data.to_path_buf(), self.keys.clone());
        let resolve: forge_core::gateway::Resolve = Arc::new(move |id: &str| {
            let profile = Settings::load(&dir).profiles.into_iter().find(|p| p.id == id)?;
            let key = if profile.auth == "none" {
                None
            } else {
                let cached = keys.lock().unwrap().get(id).cloned();
                // Only a key found is kept: a missing one is asked for again next time.
                cached.or_else(|| settings::key(id).inspect(|k| {
                    keys.lock().unwrap().insert(id.to_string(), k.clone());
                }))
            };
            Some(profile.upstream(key))
        });
        match Gateway::start(resolve) {
            Ok(g) => {
                let _ = self.gateway.set(g);
            }
            Err(e) => paths::log("error", &format!("the gateway did not start: {e}")),
        }
    }

    fn save_index(&self) {
        if let Some((index, file)) = self.index.get() {
            if let Err(e) = index.save(file) {
                paths::log("warn", &format!("could not save the session index: {e}"));
            }
        }
    }

    /// Watch the project's transcripts: sessions started elsewhere appear live.
    pub fn watch_project(&self, app: &AppHandle, folder: &Path) {
        let handle = app.clone();
        let w = watch::sessions(folder, move || {
            state(&handle).emit(Envelope { session: String::new(), event: SessionEvent::SessionsChanged });
        });
        match w {
            Ok(w) => *self.watch.lock().unwrap() = Some(w),
            Err(e) => paths::log("warn", &format!("could not watch the sessions folder: {e}")),
        }
    }

    /// Every session event passes here on its way to the window.
    fn on_event(&self, e: Envelope) {
        if matches!(e.event, SessionEvent::TurnEnd { .. }) && self.focused.lock().unwrap().as_deref() != Some(e.session.as_str()) {
            if let Some(meta) = self.meta.get() {
                let _ = meta.set_unread(&e.session, true);
            }
        }
        self.emit(e);
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
    let running = state(&app).live.running_ids();
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
    tauri::async_runtime::spawn_blocking(move || {
        let st = state(&handle);
        let Some((index, _)) = st.index.get() else { return vec![] };
        let started = Instant::now();
        let mut list = sessions::list(&cwd, index, &hidden);
        if let Some(meta) = st.meta.get() {
            let marks = meta.marks();
            for s in &mut list {
                s.pinned = marks.pinned.contains(&s.id);
                s.unread = marks.unread.contains(&s.id);
            }
        }
        st.save_index();
        let ms = started.elapsed().as_millis();
        if ms > 50 {
            paths::log("warn", &format!("listing {} sessions took {ms} ms", list.len()));
        }
        list
    })
    .await
    .map_err(|e| e.to_string())
}

/// Sessions whose messages contain the text (titles are filtered in the window).
#[tauri::command]
pub async fn sessions_search(app: AppHandle, query: String) -> Result<Vec<String>, String> {
    let Ok(cwd) = project(&app) else { return Ok(vec![]) };
    if query.trim().chars().count() < 2 || query.len() > 200 {
        return Ok(vec![]);
    }
    tauri::async_runtime::spawn_blocking(move || sessions::search(&cwd, &query, 200)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn session_pin(app: AppHandle, id: String, pinned: bool) -> Result<(), String> {
    check_id(&id)?;
    state(&app).meta.get().ok_or("not ready")?.set_pinned(&id, pinned)
}

/// The session the window now shows (None: none). It is read from now on.
#[tauri::command]
pub fn session_focus(app: AppHandle, id: Option<String>) -> Result<(), String> {
    if let Some(id) = &id {
        check_id(id)?;
        if let Some(meta) = state(&app).meta.get() {
            meta.set_unread(id, false)?;
        }
    }
    *state(&app).focused.lock().unwrap() = id;
    Ok(())
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
    state(app).live.get(id)
}

/// What a new process for a session starts with.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Launch {
    model: Option<String>,
    mode: Option<String>,
    /// "auto", "off", "low", "medium" or "high".
    thinking: Option<String>,
    /// Start as a copy of this saved session (fork).
    fork_from: Option<String>,
    /// The provider: a profile id, "" for the Claude account; None: the default.
    route: Option<String>,
}

/// Start the session's CLI (resuming its transcript if it has one) without
/// sending anything. Returns the live session.
async fn start(app: &AppHandle, id: &str, launch: Launch) -> Result<Arc<Session>, String> {
    check_id(id)?;
    let settings = Settings::load(&data_dir(app)?);
    let profile = match launch.route.as_deref() {
        Some("") => None,
        Some(r) => Some(settings.profiles.iter().find(|p| p.id == r).cloned().ok_or("that provider is not connected")?),
        None => settings.active().cloned(),
    };
    let route = profile.as_ref().map(|p| p.id.clone()).unwrap_or_default();
    if let Some(s) = running(app, id) {
        if state(app).routes.lock().unwrap().get(id).is_none_or(|r| *r == route) {
            return Ok(s);
        }
        // Another provider: the CLI restarts on the same transcript.
        if s.is_busy() || s.needs_you() {
            return Err("finish or stop this turn before switching provider".into());
        }
        state(app).live.remove(id);
        s.stop();
    }
    let cwd = project(app)?;
    let binary = cli::find_claude().ok_or("Claude Code is not installed: no `claude` on PATH")?;
    let model = match &profile {
        // Through the gateway the model carries the provider's prefix; none: the provider's default.
        Some(p) => launch.model.filter(|m| cli::valid_model(m)).map(|m| if m.starts_with(&format!("{}/", p.id)) { m } else { format!("{}/{m}", p.id) }),
        None => launch.model.filter(|m| cli::valid_model(m)).or(settings.default_model.clone()),
    };
    let mode = launch.mode.filter(|m| cli::MODES.contains(&m.as_str())).unwrap_or(settings.default_mode.clone());
    let thinking = match launch.thinking.as_deref() {
        Some(t) => Thinking::parse(t)?,
        None => None,
    };
    if let Some(f) = &launch.fork_from {
        check_id(f)?;
    }
    let (env, env_remove) = match &profile {
        Some(p) => {
            let st = state(app);
            let gw = st.gateway.get().ok_or("the Forge gateway is not running; see the log")?;
            (gw.env(&p.id, &p.model, &p.fast_model), Gateway::ENV_REMOVE.iter().map(|v| v.to_string()).collect())
        }
        None => (vec![], vec![]),
    };
    let handle = app.clone();
    let id = id.to_string();
    let session = tauri::async_runtime::spawn_blocking(move || {
        let file = sessions::file(&cwd, &id);
        let fork_from = launch.fork_from.filter(|f| sessions::file(&cwd, f).is_some());
        // A title is asked for once: for a new session, or one that has none of its own yet.
        let named = file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| sessions::summarize(&id, &t, std::time::SystemTime::now())).is_some_and(|s| s.named);
        let opts = cli::Options {
            binary,
            cwd,
            resume: file.is_some() && fork_from.is_none(),
            want_title: !named && fork_from.is_none(),
            fork_from,
            session_id: id,
            model,
            // Always explicit: the CLI's own default can be `auto` (it is when
            // nonessential traffic is off), and the window must not say "Ask" then.
            permission_mode: Some(mode),
            env,
            env_remove,
            thinking,
        };
        let sink_app = handle.clone();
        Session::start(opts, Arc::new(move |e| state(&sink_app).on_event(e)))
    })
    .await
    .map_err(|e| e.to_string())??;
    state(app).routes.lock().unwrap().insert(session.id.clone(), route);
    state(app).live.insert(session.clone());
    Ok(session)
}

/// Start a session's CLI ahead of its first message (the composer got focus,
/// a session was opened), or resume one that stopped.
#[tauri::command]
pub async fn session_prewarm(app: AppHandle, id: String, launch: Launch) -> Result<(), String> {
    start(&app, &id, launch).await.map(|_| ())
}

/// A new session that starts as a copy of `from`; returns its id. Its
/// transcript is written with its first message.
#[tauri::command]
pub async fn session_fork(app: AppHandle, from: String) -> Result<String, String> {
    check_id(&from)?;
    let cwd = project(&app)?;
    if sessions::file(&cwd, &from).is_none() {
        return Err("only a saved conversation can be forked".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    start(&app, &id, Launch { fork_from: Some(from), ..Default::default() }).await?;
    Ok(id)
}

/// Send a message, starting (or resuming) the session's CLI if it is not running.
#[tauri::command]
pub async fn session_send(app: AppHandle, id: String, text: String, launch: Launch) -> Result<(), String> {
    check_id(&id)?;
    let session = start(&app, &id, launch).await?;
    session.send(&text)
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

/// Change a running session's mode, model or thinking; a stopped one takes
/// them on its next start.
#[tauri::command]
pub fn session_configure(app: AppHandle, id: String, mode: Option<String>, model: Option<String>, thinking: Option<String>) -> Result<(), String> {
    check_id(&id)?;
    let Some(s) = running(&app, &id) else { return Ok(()) };
    if let Some(mode) = mode {
        s.set_mode(&mode)?;
    }
    if let Some(model) = model {
        s.set_model(&model)?;
    }
    if let Some(t) = thinking {
        s.set_thinking(Thinking::parse(&t)?)?;
    }
    Ok(())
}

#[tauri::command]
pub fn session_stop(app: AppHandle, id: String) -> Result<(), String> {
    check_id(&id)?;
    if let Some(s) = state(&app).live.remove(&id) {
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
    let saved = settings::set_key(&id, key.as_deref());
    // After the write: a request in between must not cache the old key.
    state(&app).keys.lock().unwrap().remove(&id);
    state(&app).models.lock().unwrap().forget(&id);
    saved
}

// ---------------------------------------------------------------- providers

#[tauri::command]
pub fn provider_catalog(app: AppHandle) -> Result<Vec<catalog::Preset>, String> {
    Ok(catalog::presets(&data_dir(&app)?))
}

fn models_url_for(app: &AppHandle, p: &settings::Profile) -> Option<String> {
    let preset = p.preset.as_deref()?;
    catalog::presets(&data_dir(app).ok()?).into_iter().find(|c| c.id == preset)?.models_url
}

/// Check a provider before it is saved: list its models with the key given
/// (or, for a saved profile, the key in the keychain). The models come back.
#[tauri::command]
pub async fn provider_check(app: AppHandle, profile: settings::Profile, key: Option<String>) -> Result<Vec<catalog::ModelInfo>, String> {
    let ok_url = (profile.base_url.starts_with("http://") || profile.base_url.starts_with("https://")) && profile.base_url.len() <= 500;
    if !ok_url || key.as_ref().is_some_and(|k| k.len() > 4096) {
        return Err("the base URL must start with http:// or https://".into());
    }
    let key = key.filter(|k| !k.trim().is_empty()).or_else(|| settings::valid_profile_id(&profile.id).then(|| settings::key(&profile.id)).flatten());
    if profile.auth != "none" && key.is_none() {
        return Err("add a key first".into());
    }
    let url = models_url_for(&app, &profile);
    let http = state(&app).http.clone();
    let models = catalog::list_models(&http, &profile.wire, &profile.base_url, url.as_deref(), &profile.auth, key.as_deref()).await?;
    paths::log("info", &format!("provider check: {} lists {} models", profile.base_url, models.len()));
    Ok(models)
}

/// A connected provider's models: cached for a day unless `refresh`.
#[tauri::command]
pub async fn provider_models(app: AppHandle, id: String, refresh: bool) -> Result<Vec<catalog::ModelInfo>, String> {
    if !refresh {
        if let Some(m) = state(&app).models.lock().unwrap().fresh(&id) {
            return Ok(m);
        }
    }
    let data = data_dir(&app)?;
    let profile = Settings::load(&data).profiles.into_iter().find(|p| p.id == id).ok_or("that provider is not connected")?;
    let key = if profile.auth == "none" { None } else { settings::key(&id) };
    let url = models_url_for(&app, &profile);
    let http = state(&app).http.clone();
    let models = catalog::list_models(&http, &profile.wire, &profile.base_url, url.as_deref(), &profile.auth, key.as_deref()).await?;
    let st = state(&app);
    let mut cache = st.models.lock().unwrap();
    cache.put(&id, models.clone());
    let _ = cache.save(&data);
    Ok(models)
}

/// Local model servers answering on their default ports now.
#[tauri::command]
pub async fn providers_detect(app: AppHandle) -> Result<Vec<catalog::Detected>, String> {
    let presets = catalog::presets(&data_dir(&app)?);
    let http = state(&app).http.clone();
    Ok(catalog::detect_local(&http, &presets).await)
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

// ---------------------------------------------------------------- Claude Code settings

fn open_project_dir(app: &AppHandle) -> Option<PathBuf> {
    state(app).project.lock().unwrap().clone()
}

#[tauri::command]
pub fn cc_settings_get(app: AppHandle) -> cc_settings::AllScopes {
    cc_settings::read_all(open_project_dir(&app).as_deref())
}

#[tauri::command]
pub fn cc_settings_set(app: AppHandle, scope: String, key: String, value: Value) -> Result<(), String> {
    let scope = cc_settings::Scope::parse(&scope)?;
    paths::log("info", &format!("settings: {key} in {scope:?}"));
    cc_settings::set(scope, open_project_dir(&app).as_deref(), &key, value)
}

/// A hook makes the CLI run a command, so the person confirms the exact
/// command in a native dialog before it is written.
#[tauri::command]
pub async fn hook_add(app: AppHandle, scope: String, event: String, matcher: String, command: String, timeout: Option<u32>) -> Result<bool, String> {
    let scope = cc_settings::Scope::parse(&scope)?;
    cc_settings::check_hook(&event, &matcher, &command, timeout)?;
    use tauri_plugin_dialog::{MessageDialogButtons, MessageDialogKind};
    let when = if matcher.is_empty() { event.clone() } else { format!("{event} ({matcher})") };
    let ok = app
        .dialog()
        .message(format!("Claude Code will run this command on {when}:\n\n{command}\n\nOnly add commands you trust."))
        .title("Add a hook?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Add hook".into(), "Cancel".into()))
        .blocking_show();
    if !ok {
        return Ok(false);
    }
    paths::log("info", &format!("hook added on {when}: {command}"));
    cc_settings::add_hook(scope, open_project_dir(&app).as_deref(), &event, &matcher, &command, timeout)?;
    Ok(true)
}

/// Approving a project MCP server makes the CLI start its command, so the
/// person confirms that command natively first.
#[tauri::command]
pub async fn mcp_approve(app: AppHandle, scope: String, name: String) -> Result<bool, String> {
    let scope = cc_settings::Scope::parse(&scope)?;
    let project = open_project_dir(&app).ok_or("no project is open")?;
    let target = cc_settings::project_mcp_target(&project, &name)?;
    use tauri_plugin_dialog::{MessageDialogButtons, MessageDialogKind};
    let ok = app
        .dialog()
        .message(format!("This project's .mcp.json defines the server \"{name}\". Claude Code will start it with:\n\n{target}\n\nOnly approve servers you trust."))
        .title("Approve an MCP server?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Approve".into(), "Cancel".into()))
        .blocking_show();
    if !ok {
        return Ok(false);
    }
    paths::log("info", &format!("MCP server approved: {name} ({target})"));
    cc_settings::approve_mcp(scope, Some(&project), &name)?;
    Ok(true)
}

#[tauri::command]
pub fn hook_remove(app: AppHandle, scope: String, event: String, group: usize, index: usize) -> Result<(), String> {
    let scope = cc_settings::Scope::parse(&scope)?;
    cc_settings::remove_hook(scope, open_project_dir(&app).as_deref(), &event, group, index)
}

#[tauri::command]
pub fn memory_read(app: AppHandle, which: String) -> Result<String, String> {
    cc_settings::memory_read(cc_settings::Memory::parse(&which)?, open_project_dir(&app).as_deref())
}

#[tauri::command]
pub fn memory_write(app: AppHandle, which: String, text: String) -> Result<(), String> {
    cc_settings::memory_write(cc_settings::Memory::parse(&which)?, open_project_dir(&app).as_deref(), &text)
}

#[tauri::command]
pub async fn extensions_list(app: AppHandle) -> Result<cc_settings::Extensions, String> {
    let project = open_project_dir(&app);
    tauri::async_runtime::spawn_blocking(move || cc_settings::extensions(project.as_deref())).await.map_err(|e| e.to_string())
}

/// Open a settings, memory or extension file in its default app: only files
/// under `~/.claude`, the project's `.claude`, or the project's CLAUDE files.
#[tauri::command]
pub fn open_file(app: AppHandle, path: String) -> Result<(), String> {
    let p = PathBuf::from(&path).canonicalize().map_err(|_| "no such file")?;
    let mut allowed: Vec<PathBuf> = cli::config_dir().into_iter().collect();
    if let Some(proj) = open_project_dir(&app) {
        allowed.push(proj.join(".claude"));
        allowed.push(proj.join("CLAUDE.md"));
        allowed.push(proj.join("CLAUDE.local.md"));
        allowed.push(proj.join(".mcp.json"));
    }
    let ok = allowed.iter().filter_map(|a| a.canonicalize().ok()).any(|a| p.starts_with(&a));
    if !ok || !p.is_file() {
        return Err("only Claude Code's own files open from here".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_path(p.display().to_string(), None::<&str>).map_err(|e| e.to_string())
}
