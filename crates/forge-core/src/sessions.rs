//! A project's saved conversations: the CLI's own JSONL files under
//! `~/.claude/projects/<slug>/`, shared with the terminal and other editors.
//!
//! Listing reads each file once per change (by size and mtime) for its title
//! and last activity. Loading turns one file into the messages the window
//! shows. Renaming appends the same `custom-title` line the CLI's /rename
//! writes; nothing else here writes to the CLI's files.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};

use crate::cli;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    /// Milliseconds since the epoch.
    pub updated: u64,
    pub branch: Option<String>,
}

/// The most messages one load returns: the newest ones.
const MAX_LOADED: usize = 3000;

#[derive(Default)]
pub struct Cache(Mutex<HashMap<PathBuf, (u64, SystemTime, Option<Summary>)>>);

/// The project's conversation folder. Claude Code names it from the path; a
/// path too long for that is found by the `cwd` its files record.
pub fn folder(cwd: &Path) -> Option<PathBuf> {
    let root = cli::projects_dir()?;
    let named = root.join(cli::project_slug(cwd));
    if named.is_dir() {
        return Some(named);
    }
    let want = cwd.to_string_lossy().to_string();
    fs::read_dir(&root).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).find(|dir| {
        fs::read_dir(dir).ok().into_iter().flatten().flatten().take(3).any(|f| {
            fs::read_to_string(f.path())
                .ok()
                .and_then(|s| s.lines().find_map(|l| serde_json::from_str::<Value>(l).ok()?.get("cwd")?.as_str().map(String::from)))
                .is_some_and(|c| c == want)
        })
    })
}

pub fn file(cwd: &Path, id: &str) -> Option<PathBuf> {
    if !cli::valid_session_id(id) {
        return None;
    }
    let path = folder(cwd)?.join(format!("{id}.jsonl"));
    path.is_file().then_some(path)
}

pub fn list(cwd: &Path, cache: &Cache, hidden: &[String]) -> Vec<Summary> {
    let Some(dir) = folder(cwd) else { return vec![] };
    let mut out = vec![];
    let mut cache = cache.0.lock().unwrap();
    for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|s| cli::valid_session_id(s)).map(String::from) else { continue };
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") || hidden.contains(&id) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
        let fresh = cache.get(&path).is_some_and(|(len, t, _)| *len == meta.len() && *t == mtime);
        if !fresh {
            let summary = fs::read_to_string(&path).ok().and_then(|text| summarize(&id, &text, mtime));
            cache.insert(path.clone(), (meta.len(), mtime, summary));
        }
        if let Some((_, _, Some(s))) = cache.get(&path) {
            out.push(s.clone());
        }
    }
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    out
}

fn millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn summarize(id: &str, text: &str, mtime: SystemTime) -> Option<Summary> {
    let mut custom = None;
    let mut summary = None;
    let mut first = None;
    let mut branch = None;
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        match v.get("type").and_then(Value::as_str) {
            Some("custom-title") => custom = v.get("customTitle").and_then(Value::as_str).map(String::from),
            Some("summary") => summary = v.get("summary").and_then(Value::as_str).map(String::from),
            Some("user") if first.is_none() && v.get("isMeta") != Some(&json!(true)) && v.get("isSidechain") != Some(&json!(true)) => {
                first = v.get("message").and_then(|m| m.get("content")).and_then(crate::events::typed_text);
                if branch.is_none() {
                    branch = v.get("gitBranch").and_then(Value::as_str).filter(|b| *b != "HEAD").map(String::from);
                }
            }
            _ => {}
        }
    }
    let first = first?;
    let title = custom.or(summary).unwrap_or_else(|| {
        let line = first.lines().next().unwrap_or_default();
        let mut t: String = line.chars().take(80).collect();
        if line.chars().count() > 80 {
            t.push('…');
        }
        t
    });
    Some(Summary { id: id.to_string(), title, updated: millis(mtime), branch })
}

/// The conversation as the window shows it: user and assistant messages in
/// order, each `{role, content, timestamp}` with the CLI's content blocks.
pub fn load(path: &Path) -> Result<Vec<Value>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out: Vec<Value> = vec![];
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let role = match v.get("type").and_then(Value::as_str) {
            Some(r @ ("user" | "assistant")) => r,
            _ => continue,
        };
        if v.get("isSidechain") == Some(&json!(true)) || v.get("isMeta") == Some(&json!(true)) {
            continue;
        }
        let Some(message) = v.get("message") else { continue };
        let content = message.get("content").cloned().unwrap_or(Value::Null);
        if role == "user" {
            if let Value::String(s) = &content {
                if s.trim_start().starts_with('<') {
                    continue;
                }
            }
        }
        out.push(json!({
            "role": role,
            "id": message.get("id").cloned().unwrap_or(Value::Null),
            "content": content,
            "timestamp": v.get("timestamp").cloned().unwrap_or(Value::Null),
        }));
    }
    if out.len() > MAX_LOADED {
        out.drain(..out.len() - MAX_LOADED);
    }
    Ok(out)
}

/// Name a conversation the way the CLI's /rename does.
pub fn rename(path: &Path, id: &str, title: &str) -> Result<(), String> {
    let title: String = title.trim().chars().filter(|c| !c.is_control()).take(120).collect();
    if title.is_empty() {
        return Err("a title cannot be empty".into());
    }
    let line = serde_json::to_string(&json!({"type": "custom-title", "customTitle": title, "sessionId": id})).map_err(|e| e.to_string())?;
    let mut f = fs::OpenOptions::new().append(true).open(path).map_err(|e| e.to_string())?;
    // Start on a fresh line even if the file's last line has no newline.
    let ends_clean = fs::read(path).map(|b| b.last().is_none_or(|c| *c == b'\n')).unwrap_or(true);
    let prefix = if ends_clean { "" } else { "\n" };
    writeln!(f, "{prefix}{line}").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0e5b8a1c-1111-4222-8333-444455556666";

    fn sample() -> String {
        [
            r#"{"type":"queue-operation","sessionId":"x"}"#,
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>x</local-command-caveat>"}}"#,
            r#"{"type":"user","gitBranch":"main","message":{"role":"user","content":"Fix the login bug\nand add a test"},"timestamp":"t1"}"#,
            r#"{"type":"assistant","message":{"id":"m1","role":"assistant","content":[{"type":"text","text":"On it."}]},"timestamp":"t2"}"#,
            r#"{"type":"assistant","message":{"id":"m1","role":"assistant","content":[{"type":"tool_use","id":"tu1","name":"Read","input":{"file_path":"a"}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tu1","content":"ok"}]}}"#,
            r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":"agent work"}}"#,
            "not json",
        ]
        .join("\n")
    }

    #[test]
    fn title_from_the_first_typed_message_then_renames() {
        let s = summarize(ID, &sample(), UNIX_EPOCH).unwrap();
        assert_eq!(s.title, "Fix the login bug");
        assert_eq!(s.branch.as_deref(), Some("main"));
        let renamed = format!("{}\n{}", sample(), r#"{"type":"custom-title","customTitle":"Login fix","sessionId":"x"}"#);
        assert_eq!(summarize(ID, &renamed, UNIX_EPOCH).unwrap().title, "Login fix");
        assert!(summarize(ID, r#"{"type":"queue-operation"}"#, UNIX_EPOCH).is_none(), "nothing typed, nothing listed");
    }

    #[test]
    fn loads_the_conversation_and_renames_in_place() {
        let dir = std::env::temp_dir().join(format!("forge-sessions-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{ID}.jsonl"));
        fs::write(&path, sample()).unwrap();
        let msgs = load(&path).unwrap();
        let roles: Vec<&str> = msgs.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["user", "assistant", "assistant", "user"]);
        assert_eq!(msgs[1]["content"][0]["text"], "On it.");

        rename(&path, ID, "  Login\u{7} fix  ").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'));
        let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(last, json!({"type": "custom-title", "customTitle": "Login fix", "sessionId": ID}));
        assert_eq!(summarize(ID, &text, UNIX_EPOCH).unwrap().title, "Login fix");
        assert!(rename(&path, ID, "   ").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
