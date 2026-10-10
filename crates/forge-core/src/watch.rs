//! Watches Claude Code's conversation folder, so a session started or
//! continued in the terminal shows up in the window at once, without polling.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::{cli, log};

/// Dropping it stops the watch.
pub struct Watch {
    _watcher: RecommendedWatcher,
}

/// Call `changed` (at most every 250 ms) when a transcript of `cwd`'s
/// project is created, grows or goes away.
pub fn sessions(cwd: &Path, changed: impl Fn() + Send + 'static) -> Result<Watch, String> {
    let root = cli::projects_dir().ok_or("no home folder")?;
    // The CLI creates it with the first session; watching needs it now.
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let slug = cli::project_slug(cwd);
    let cwd: PathBuf = cwd.to_path_buf();
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        let ours = event.paths.iter().any(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("jsonl")
                && p.parent().and_then(|d| d.file_name()).is_some_and(|d| d.to_string_lossy() == slug || crate::sessions::folder(&cwd).is_some_and(|f| Some(f.as_path()) == p.parent()))
        });
        if ours {
            let _ = tx.send(());
        }
    })
    .map_err(|e| e.to_string())?;
    watcher.watch(&root, RecursiveMode::Recursive).map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("session-watch".into())
        .spawn(move || {
            while rx.recv().is_ok() {
                // Coalesce a burst (a streaming reply appends many lines).
                std::thread::sleep(Duration::from_millis(250));
                while rx.try_recv().is_ok() {}
                changed();
            }
        })
        .map_err(|e| e.to_string())?;
    log::write("info", &format!("watching {}", root.display()));
    Ok(Watch { _watcher: watcher })
}
