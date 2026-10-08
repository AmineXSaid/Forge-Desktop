//! Where things are, and the shell's log.
//!
//! - resources: the staged Forge root (`stage/forge`, bundled as `forge/`);
//! - data: `%APPDATA%\com.msaid.forge` (settings.json, state.json, recent.json);
//! - logs: `%LOCALAPPDATA%\com.msaid.forge\logs` (forge-desktop.log, forge-host.log).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

static LOG: OnceLock<Mutex<Option<fs::File>>> = OnceLock::new();
const LOG_LIMIT: u64 = 5 * 1024 * 1024;

pub struct Dirs {
    pub resources: PathBuf,
    pub data: PathBuf,
    pub logs: PathBuf,
}

pub fn dirs(app: &AppHandle) -> Result<Dirs, String> {
    let path = app.path();
    let resources = match std::env::var_os("FORGE_RESOURCES").map(PathBuf::from) {
        Some(p) => p,
        // `tauri dev` runs from the source tree: use the staged copy in place.
        None if cfg!(debug_assertions) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("stage").join("forge"),
        None => path.resource_dir().map_err(|e| e.to_string())?.join("forge"),
    };
    Ok(Dirs {
        resources,
        data: path.app_data_dir().map_err(|e| e.to_string())?,
        logs: path.app_log_dir().map_err(|e| e.to_string())?,
    })
}

pub fn init_log(dir: &PathBuf) {
    let _ = fs::create_dir_all(dir);
    let file = dir.join("forge-desktop.log");
    if fs::metadata(&file).map(|m| m.len() > LOG_LIMIT).unwrap_or(false) {
        let _ = fs::rename(&file, dir.join("forge-desktop.1.log"));
    }
    let handle = OpenOptions::new().create(true).append(true).open(&file).ok();
    let _ = LOG.set(Mutex::new(handle));
}

pub fn log(level: &str, message: &str) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let line = format!("{secs:.3} [{level}] {message}\n");
    if let Some(Ok(mut guard)) = LOG.get().map(|m| m.lock()) {
        if let Some(file) = guard.as_mut() {
            if file.write_all(line.as_bytes()).is_ok() {
                return;
            }
        }
    }
    eprint!("{line}");
}
