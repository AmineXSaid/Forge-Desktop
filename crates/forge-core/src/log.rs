//! The app's file log: one line per event, rotated at 5 MB. The CLI's stderr
//! goes here too. Without `init`, lines go to stderr.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG: OnceLock<Mutex<Option<fs::File>>> = OnceLock::new();
const LOG_LIMIT: u64 = 5 * 1024 * 1024;

pub fn init(dir: &Path) {
    let _ = fs::create_dir_all(dir);
    let file = dir.join("forge-desktop.log");
    if fs::metadata(&file).map(|m| m.len() > LOG_LIMIT).unwrap_or(false) {
        let _ = fs::rename(&file, dir.join("forge-desktop.1.log"));
    }
    let handle = OpenOptions::new().create(true).append(true).open(&file).ok();
    let _ = LOG.set(Mutex::new(handle));
}

pub fn write(level: &str, message: &str) {
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
