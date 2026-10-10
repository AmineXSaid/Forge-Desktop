//! Where the app keeps things:
//! - data: `%APPDATA%\com.msaid.forge` (settings.json, recent.json);
//! - logs: `%LOCALAPPDATA%\com.msaid.forge\logs` (forge-desktop.log, with the CLI's stderr).

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

pub use forge_core::log::write as log;

pub struct Dirs {
    pub data: PathBuf,
    pub logs: PathBuf,
}

pub fn dirs(app: &AppHandle) -> Result<Dirs, String> {
    let path = app.path();
    Ok(Dirs {
        data: path.app_data_dir().map_err(|e| e.to_string())?,
        logs: path.app_log_dir().map_err(|e| e.to_string())?,
    })
}
