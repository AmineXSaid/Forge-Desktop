//! A project's saved conversations: the CLI's own JSONL files under
//! `~/.claude/projects/<slug>/`, shared with the terminal and other editors.
//!
//! Listing goes through an `Index` kept on disk between launches: for each
//! file, its size, mtime, how many bytes have been read, and what was found
//! there (first message, titles, branch). A file that grew is read only from
//! where the last read stopped, and only lines that can matter are parsed,
//! so a list of hundreds of sessions costs one `stat` each. Loading turns one
//! file into the messages the window shows. Renaming appends the same
//! `custom-title` line the CLI's /rename writes; nothing else here writes to
//! the CLI's files.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
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
    /// It has a title of its own (renamed, or generated), not just its first line.
    pub named: bool,
    /// The app's own marks (`meta.rs`), filled in by the app shell.
    pub pinned: bool,
    pub unread: bool,
}

/// The most messages one load returns: the newest ones.
const MAX_LOADED: usize = 3000;

/// What one file holds, as far as it has been read.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
struct Entry {
    len: u64,
    mtime: u64,
    /// Bytes of complete lines read so far.
    parsed: u64,
    first: Option<String>,
    branch: Option<String>,
    custom: Option<String>,
    ai: Option<String>,
    summary: Option<String>,
}

impl Entry {
    /// Complete lines only; a partly written last line waits for the next read.
    fn feed(&mut self, text: &str) {
        for line in text.lines() {
            // Cheap checks first: most lines are messages and tool output.
            let wanted = line.contains("\"type\":\"custom-title\"")
                || line.contains("\"type\":\"ai-title\"")
                || line.contains("\"type\":\"summary\"")
                || (self.first.is_none() && line.contains("\"type\":\"user\""));
            if !wanted {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            let text_of = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
            match v.get("type").and_then(Value::as_str) {
                Some("custom-title") => self.custom = text_of("customTitle"),
                Some("ai-title") => self.ai = text_of("aiTitle"),
                Some("summary") => self.summary = text_of("summary"),
                Some("user") if self.first.is_none() && v.get("isMeta") != Some(&json!(true)) && v.get("isSidechain") != Some(&json!(true)) => {
                    self.first = v.get("message").and_then(|m| m.get("content")).and_then(crate::events::typed_text);
                    if self.first.is_some() {
                        self.branch = v.get("gitBranch").and_then(Value::as_str).filter(|b| *b != "HEAD").map(String::from);
                    }
                }
                _ => {}
            }
        }
    }

    fn summary(&self, id: &str) -> Option<Summary> {
        let first = self.first.as_ref()?;
        let named = self.custom.is_some() || self.ai.is_some();
        let title = self.custom.clone().or(self.ai.clone()).or(self.summary.clone()).unwrap_or_else(|| {
            let line = first.lines().next().unwrap_or_default();
            let mut t: String = line.chars().take(80).collect();
            if line.chars().count() > 80 {
                t.push('…');
            }
            t
        });
        Some(Summary { id: id.to_string(), title, updated: self.mtime, branch: self.branch.clone(), named, pinned: false, unread: false })
    }
}

/// Every listed file's `Entry`, by path. Saved to the app's data folder.
#[derive(Default)]
pub struct Index {
    entries: Mutex<HashMap<String, Entry>>,
    dirty: AtomicBool,
}

impl Index {
    pub fn load(file: &Path) -> Index {
        let entries = fs::read(file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Index { entries: Mutex::new(entries), dirty: AtomicBool::new(false) }
    }

    /// Write it out if anything changed since the last save.
    pub fn save(&self, file: &Path) -> Result<(), String> {
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&*self.entries.lock().unwrap()).map_err(|e| e.to_string())?;
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = file.with_extension("json.tmp");
        fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        fs::rename(tmp, file).map_err(|e| e.to_string())
    }
}

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

/// Bring one entry up to date with its file: read only what was appended.
fn refresh(path: &Path, len: u64, mtime: u64, entry: &mut Entry) -> std::io::Result<()> {
    if len < entry.parsed {
        // Rewritten or truncated: start over.
        *entry = Entry::default();
    }
    if len > entry.parsed {
        let mut f = fs::File::open(path)?;
        f.seek(SeekFrom::Start(entry.parsed))?;
        let mut buf = Vec::with_capacity((len - entry.parsed) as usize);
        f.take(len - entry.parsed).read_to_end(&mut buf)?;
        let complete = buf.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        entry.feed(&String::from_utf8_lossy(&buf[..complete]));
        entry.parsed += complete as u64;
    }
    entry.len = len;
    entry.mtime = mtime;
    Ok(())
}

pub fn list(cwd: &Path, index: &Index, hidden: &[String]) -> Vec<Summary> {
    let Some(dir) = folder(cwd) else { return vec![] };
    let mut out = vec![];
    let mut entries = index.entries.lock().unwrap();
    for item in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = item.path();
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|s| cli::valid_session_id(s)).map(String::from) else { continue };
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") || hidden.contains(&id) {
            continue;
        }
        let Ok(meta) = item.metadata() else { continue };
        let mtime = millis(meta.modified().unwrap_or(UNIX_EPOCH));
        let key = path.to_string_lossy().to_string();
        let entry = entries.entry(key).or_default();
        if entry.len != meta.len() || entry.mtime != mtime {
            if refresh(&path, meta.len(), mtime, entry).is_err() {
                continue;
            }
            index.dirty.store(true, Ordering::Relaxed);
        }
        if let Some(s) = entry.summary(&id) {
            out.push(s);
        }
    }
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    out
}

/// Sessions whose typed messages or replies contain `query` (any case), newest first.
pub fn search(cwd: &Path, query: &str, limit: usize) -> Vec<String> {
    let q = query.trim().to_lowercase();
    let Some(dir) = folder(cwd) else { return vec![] };
    if q.is_empty() {
        return vec![];
    }
    let mut hits: Vec<(SystemTime, String)> = vec![];
    for item in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = item.path();
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|s| cli::valid_session_id(s)).map(String::from) else { continue };
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let found = text.lines().any(|line| {
            // The raw line first (cheap); then only the message's own text, so a
            // match in a path, an id or tool output does not count.
            if !line.to_lowercase().contains(&q) || !(line.contains("\"type\":\"user\"") || line.contains("\"type\":\"assistant\"")) {
                return false;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
            let content = v.get("message").and_then(|m| m.get("content")).unwrap_or(&Value::Null);
            let text = match content {
                Value::String(s) => s.clone(),
                Value::Array(blocks) => blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
                _ => String::new(),
            };
            text.to_lowercase().contains(&q)
        });
        if found {
            hits.push((item.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH), id));
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    hits.into_iter().take(limit).map(|(_, id)| id).collect()
}

fn millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn summarize(id: &str, text: &str, mtime: SystemTime) -> Option<Summary> {
    let mut e = Entry { mtime: millis(mtime), ..Default::default() };
    e.feed(text);
    e.summary(id)
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
    fn generated_titles_and_appended_lines() {
        let with_ai = format!("{}\n{}", sample(), r#"{"type":"ai-title","aiTitle":"Fix login","sessionId":"x"}"#);
        let s = summarize(ID, &with_ai, UNIX_EPOCH).unwrap();
        assert_eq!((s.title.as_str(), s.named), ("Fix login", true));
        let renamed = format!("{with_ai}\n{}", r#"{"type":"custom-title","customTitle":"Mine","sessionId":"x"}"#);
        assert_eq!(summarize(ID, &renamed, UNIX_EPOCH).unwrap().title, "Mine", "a rename wins over a generated title");
        assert!(!summarize(ID, &sample(), UNIX_EPOCH).unwrap().named);

        // The index reads only what was appended, and waits for a line's end.
        let dir = std::env::temp_dir().join(format!("forge-index-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{ID}.jsonl"));
        fs::write(&path, format!("{}\n", sample())).unwrap();
        let mut e = Entry::default();
        let len = |p: &Path| fs::metadata(p).unwrap().len();
        refresh(&path, len(&path), 1, &mut e).unwrap();
        assert_eq!(e.summary(ID).unwrap().title, "Fix the login bug");
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(f, "{}", r#"{"type":"ai-title","aiTitle":"Half"#).unwrap();
        refresh(&path, len(&path), 2, &mut e).unwrap();
        assert_eq!(e.summary(ID).unwrap().title, "Fix the login bug", "a half-written line is not read");
        writeln!(f, "{}", r#" done","sessionId":"x"}"#).unwrap();
        refresh(&path, len(&path), 3, &mut e).unwrap();
        assert_eq!(e.summary(ID).unwrap().title, "Half done");
        fs::write(&path, r#"{"type":"user","message":{"content":"Other"}}"#.to_string() + "\n").unwrap();
        refresh(&path, len(&path), 4, &mut e).unwrap();
        assert_eq!(e.summary(ID).unwrap().title, "Other", "a shorter file is read again from the start");
        let _ = fs::remove_dir_all(&dir);
    }

    /// ARCHITECTURE.md §5: a 500-session list in 50 ms, warm or from the saved index.
    #[test]
    fn five_hundred_sessions_list_within_budget() {
        let _guard = crate::cli::tests_env::lock();
        let root = std::env::temp_dir().join(format!("forge-bench-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let cwd = root.join("proj");
        let config = root.join("config");
        let dir = config.join("projects").join(cli::project_slug(&cwd));
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&cwd).unwrap();
        let body: String = (0..200).map(|i| format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": format!("reply {i} with some words to make the line realistic")}]}}))).collect();
        for n in 0..500 {
            let id = format!("00000000-0000-4000-8000-{n:012}");
            let first = json!({"type": "user", "gitBranch": "main", "message": {"role": "user", "content": format!("task number {n}")}});
            fs::write(dir.join(format!("{id}.jsonl")), format!("{first}\n{body}")).unwrap();
        }
        std::env::set_var("CLAUDE_CONFIG_DIR", &config);
        let index = Index::default();
        let t = std::time::Instant::now();
        assert_eq!(list(&cwd, &index, &[]).len(), 500);
        let cold = t.elapsed();
        let t = std::time::Instant::now();
        assert_eq!(list(&cwd, &index, &[]).len(), 500);
        let warm = t.elapsed();
        let saved = root.join("index.json");
        index.save(&saved).unwrap();
        let reloaded = Index::load(&saved);
        let t = std::time::Instant::now();
        let again = list(&cwd, &reloaded, &[]);
        let after_restart = t.elapsed();
        assert!(!reloaded.dirty.load(Ordering::Relaxed), "nothing re-read after a restart");
        assert_eq!(again.len(), 500);
        assert!(search(&cwd, "TASK NUMBER 42", 50).contains(&"00000000-0000-4000-8000-000000000042".to_string()));
        assert!(search(&cwd, "proj", 10).is_empty(), "paths do not match");
        std::env::remove_var("CLAUDE_CONFIG_DIR");
        eprintln!("500 sessions: cold {cold:?}, warm {warm:?}, after restart {after_restart:?}");
        assert!(warm.as_millis() <= 50 && after_restart.as_millis() <= 50, "warm {warm:?}, after restart {after_restart:?}");
        let _ = fs::remove_dir_all(&root);
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
