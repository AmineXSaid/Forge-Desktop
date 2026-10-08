//! Node -> Rust calls: what the host needs from the desktop that Node cannot
//! do itself (native dialogs, the browser, Explorer, the window title).
//!
//! The method list is fixed (plan §4.3); `runtime.ts` `HostMethod` is the Node
//! side of it. Parameters are checked before anything runs: URLs by scheme,
//! paths must be absolute, titles have a length.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::sidecar;

/// A parsed, checked call.
#[derive(Debug, PartialEq)]
pub enum Call {
    OpenFolder { title: Option<String>, default_path: Option<PathBuf> },
    OpenFiles { title: Option<String>, default_path: Option<PathBuf>, multiple: bool, filters: Vec<(String, Vec<String>)> },
    SaveFile { title: Option<String>, default_path: Option<PathBuf>, filters: Vec<(String, Vec<String>)> },
    OpenUrl(String),
    ShowInFolder(PathBuf),
    SetTitle(String),
    AppInfo,
}

const MAX_TITLE: usize = 200;

fn opt_string(params: &Value, key: &str) -> Result<Option<String>, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.chars().count() <= MAX_TITLE => Ok(Some(s.clone())),
        Some(Value::String(_)) => Err(format!("{key} is too long")),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn absolute(value: &str, key: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(format!("{key} must be an absolute path"));
    }
    Ok(path)
}

fn opt_path(params: &Value, key: &str) -> Result<Option<PathBuf>, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => absolute(s, key).map(Some),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn filters(params: &Value) -> Result<Vec<(String, Vec<String>)>, String> {
    let Some(list) = params.get("filters").filter(|v| !v.is_null()) else { return Ok(vec![]) };
    let list = list.as_array().ok_or("filters must be a list")?;
    list.iter()
        .take(20)
        .map(|f| {
            let name = f.get("name").and_then(Value::as_str).ok_or("a filter needs a name")?;
            let exts = f.get("extensions").and_then(Value::as_array).ok_or("a filter needs extensions")?;
            let exts = exts
                .iter()
                .map(|e| {
                    e.as_str()
                        .filter(|e| !e.is_empty() && e.len() <= 32 && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '*'))
                        .map(String::from)
                        .ok_or_else(|| "a filter extension must be short and alphanumeric".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((name.chars().take(64).collect(), exts))
        })
        .collect()
}

/// The schemes `open_url` accepts, as Forge's `isOpenableUrl` does.
pub fn is_openable_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:"))
        && url.len() <= 8192
        && !url.chars().any(|c| c.is_control())
}

pub fn parse(method: &str, params: &Value) -> Result<Call, String> {
    match method {
        "dialog.openFolder" => Ok(Call::OpenFolder { title: opt_string(params, "title")?, default_path: opt_path(params, "defaultPath")? }),
        "dialog.openFiles" => Ok(Call::OpenFiles {
            title: opt_string(params, "title")?,
            default_path: opt_path(params, "defaultPath")?,
            multiple: params.get("multiple").and_then(Value::as_bool).unwrap_or(false),
            filters: filters(params)?,
        }),
        "dialog.saveFile" => Ok(Call::SaveFile {
            title: opt_string(params, "title")?,
            default_path: opt_path(params, "defaultPath")?,
            filters: filters(params)?,
        }),
        "shell.openUrl" => {
            let url = params.get("url").and_then(Value::as_str).ok_or("url must be a string")?;
            if !is_openable_url(url) {
                return Err("only http, https and mailto links open".into());
            }
            Ok(Call::OpenUrl(url.to_string()))
        }
        "shell.showInFolder" => {
            let path = params.get("path").and_then(Value::as_str).ok_or("path must be a string")?;
            Ok(Call::ShowInFolder(absolute(path, "path")?))
        }
        "window.setTitle" => Ok(Call::SetTitle(opt_string(params, "title")?.ok_or("title must be a string")?)),
        "app.info" => Ok(Call::AppInfo),
        other => Err(format!("unknown host call: {}", other.chars().take(64).collect::<String>())),
    }
}

fn path_value(path: Option<tauri_plugin_dialog::FilePath>) -> Value {
    path.and_then(|p| p.into_path().ok()).map(|p| json!(p.to_string_lossy())).unwrap_or(Value::Null)
}

fn dialog_base<R: tauri::Runtime>(
    app: &AppHandle<R>,
    title: &Option<String>,
    default_path: &Option<PathBuf>,
) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    let mut builder = app.dialog().file();
    if let Some(window) = app.get_webview_window("main") {
        builder = builder.set_parent(&window);
    }
    if let Some(t) = title {
        builder = builder.set_title(t);
    }
    if let Some(p) = default_path {
        builder = if p.is_dir() { builder.set_directory(p) } else { builder.set_directory(p.parent().unwrap_or(Path::new("/"))).set_file_name(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()) };
    }
    builder
}

/// Run one call from the host and answer it on stdin.
pub fn handle(app: &AppHandle, id: u64, method: &str, params: &Value) {
    let call = match parse(method, params) {
        Ok(call) => call,
        Err(error) => return sidecar::answer(app, id, Err(error)),
    };
    let reply = app.clone();
    match call {
        Call::OpenFolder { title, default_path } => {
            dialog_base(app, &title, &default_path).pick_folder(move |p| sidecar::answer(&reply, id, Ok(path_value(p))));
        }
        Call::OpenFiles { title, default_path, multiple, filters } => {
            let mut b = dialog_base(app, &title, &default_path);
            for (name, exts) in &filters {
                let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
                b = b.add_filter(name, &exts);
            }
            if multiple {
                b.pick_files(move |ps| {
                    let list = ps.unwrap_or_default().into_iter().filter_map(|p| p.into_path().ok()).map(|p| json!(p.to_string_lossy())).collect::<Vec<_>>();
                    sidecar::answer(&reply, id, Ok(json!(list)));
                });
            } else {
                b.pick_file(move |p| {
                    let v = path_value(p);
                    sidecar::answer(&reply, id, Ok(if v.is_null() { v } else { json!([v]) }));
                });
            }
        }
        Call::SaveFile { title, default_path, filters } => {
            let mut b = dialog_base(app, &title, &default_path);
            for (name, exts) in &filters {
                let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
                b = b.add_filter(name, &exts);
            }
            b.save_file(move |p| sidecar::answer(&reply, id, Ok(path_value(p))));
        }
        Call::OpenUrl(url) => {
            let result = app.opener().open_url(url, None::<&str>).map(|_| Value::Null).map_err(|e| e.to_string());
            sidecar::answer(app, id, result);
        }
        Call::ShowInFolder(path) => {
            let result = app.opener().reveal_item_in_dir(path).map(|_| Value::Null).map_err(|e| e.to_string());
            sidecar::answer(app, id, result);
        }
        Call::SetTitle(title) => {
            let result = match app.get_webview_window("main") {
                Some(w) => w.set_title(&title).map(|_| Value::Null).map_err(|e| e.to_string()),
                None => Err("no window".into()),
            };
            sidecar::answer(app, id, result);
        }
        Call::AppInfo => {
            let info = json!({
                "version": app.package_info().version.to_string(),
                "platform": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
            });
            sidecar::answer(app, id, Ok(info));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_open_only_for_web_and_mail() {
        assert!(is_openable_url("https://example.com/a?b"));
        assert!(is_openable_url("HTTP://example.com"));
        assert!(is_openable_url("mailto:x@example.com"));
        assert!(!is_openable_url("file:///etc/passwd"));
        assert!(!is_openable_url("javascript:alert(1)"));
        assert!(!is_openable_url("C:\\Windows\\System32\\calc.exe"));
        assert!(!is_openable_url("ms-settings:"));
        assert!(!is_openable_url("https://example.com/\nnext"));
    }

    #[test]
    fn unknown_methods_are_refused() {
        assert!(parse("shell.exec", &json!({"command": "calc"})).is_err());
        assert!(parse("secrets.get", &json!({})).is_err());
    }

    #[test]
    fn paths_must_be_absolute() {
        assert!(parse("shell.showInFolder", &json!({"path": "relative/file"})).is_err());
        assert!(parse("dialog.saveFile", &json!({"defaultPath": "../x"})).is_err());
        let abs = if cfg!(windows) { "C:\\work\\log.txt" } else { "/work/log.txt" };
        assert_eq!(parse("shell.showInFolder", &json!({"path": abs})), Ok(Call::ShowInFolder(PathBuf::from(abs))));
    }

    #[test]
    fn filters_are_checked() {
        assert!(parse("dialog.openFiles", &json!({"filters": [{"name": "JSON", "extensions": ["json"]}]})).is_ok());
        assert!(parse("dialog.openFiles", &json!({"filters": [{"name": "x", "extensions": ["../x"]}]})).is_err());
        assert!(parse("dialog.openFiles", &json!({"filters": "json"})).is_err());
    }

    #[test]
    fn titles_have_a_length() {
        assert!(parse("window.setTitle", &json!({"title": "Forge"})).is_ok());
        assert!(parse("window.setTitle", &json!({"title": "x".repeat(MAX_TITLE + 1)})).is_err());
        assert!(parse("window.setTitle", &json!({})).is_err());
    }
}
