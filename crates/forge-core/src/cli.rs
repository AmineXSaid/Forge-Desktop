//! One Claude Code CLI process per chat session, driven over stream-json.
//!
//! The CLI runs as `claude -p --input-format stream-json --output-format
//! stream-json --verbose --include-partial-messages --permission-prompt-tool
//! stdio`. Each line both ways is one JSON object:
//!
//! - to the CLI: user messages, our control requests (initialize, interrupt,
//!   set_permission_mode, set_model) and our answers to its control requests;
//! - from the CLI: transcript events (system, assistant, user, stream_event,
//!   result), answers to our requests, and its own control requests, of which
//!   `can_use_tool` is the permission prompt.
//!
//! A permission prompt is kept here with the tool's input as the CLI sent it.
//! The window only says allow, allow always, deny, or (for AskUserQuestion)
//! which answers: it never supplies the input that runs.
//!
//! Nothing here knows about Tauri: events go to a `Sink`, so the tests drive a
//! real CLI without a window.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::events::{Envelope, Normalizer, SessionEvent};
use crate::log;

pub type Sink = Arc<dyn Fn(Envelope) + Send + Sync>;

/// Permission modes the CLI accepts.
pub const MODES: &[&str] = &["default", "manual", "acceptEdits", "plan", "auto", "dontAsk", "bypassPermissions"];

/// The longest message the window may send in one turn.
pub const MAX_PROMPT_BYTES: usize = 512 * 1024;

pub struct Options {
    pub binary: PathBuf,
    pub cwd: PathBuf,
    pub session_id: String,
    /// Continue a saved conversation instead of starting one with this id.
    pub resume: bool,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub env: Vec<(String, String)>,
    /// Inherited variables to drop before `env` applies.
    pub env_remove: Vec<String>,
}

/// What the window decided about one permission prompt.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow,
    /// Allow, and apply the CLI's own suggestion (a rule, or a mode) so it
    /// does not ask again.
    AllowAlways,
    Deny(String),
    /// AskUserQuestion: question text to the chosen label(s), joined by ", ".
    Answer(Map<String, Value>),
}

struct Pending {
    tool: String,
    input: Value,
    suggestions: Value,
}

pub struct Session {
    pub id: String,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    pending: Mutex<HashMap<String, Pending>>,
    next_request: AtomicU64,
    normalizer: Mutex<Normalizer>,
}

/// `claude` on this machine: `$FORGE_CLAUDE`, else PATH, else the places the
/// native installer and the old local install put it.
pub fn find_claude() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FORGE_CLAUDE").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    let home = home_dir();
    let extra = home
        .iter()
        .flat_map(|h| [h.join(".local").join("bin"), h.join(".claude").join("local")])
        .collect::<Vec<_>>();
    on_path.iter().chain(extra.iter()).flat_map(|dir| names.iter().map(move |n| dir.join(n))).find(|p| p.is_file())
}

pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from)
}

/// The arguments for one session.
pub fn args(opts: &Options) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-prompt-tool",
        "stdio",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if opts.resume {
        args.extend(["--resume".into(), opts.session_id.clone()]);
    } else {
        args.extend(["--session-id".into(), opts.session_id.clone()]);
    }
    if let Some(model) = &opts.model {
        args.extend(["--model".into(), model.clone()]);
    }
    if let Some(mode) = &opts.permission_mode {
        args.extend(["--permission-mode".into(), mode.clone()]);
    }
    args
}

pub fn valid_session_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    parts.len() == 5
        && [8, 4, 4, 4, 12].iter().zip(&parts).all(|(n, p)| p.len() == *n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

pub fn valid_model(model: &str) -> bool {
    !model.is_empty() && model.len() <= 120 && model.chars().all(|c| c.is_ascii_alphanumeric() || "._:/-[]@".contains(c))
}

impl Session {
    pub fn start(opts: Options, sink: Sink) -> Result<Arc<Session>, String> {
        if !valid_session_id(&opts.session_id) {
            return Err("bad session id".into());
        }
        let mut command = Command::new(&opts.binary);
        for var in &opts.env_remove {
            command.env_remove(var);
        }
        command
            .args(args(&opts))
            .current_dir(&opts.cwd)
            // A Claude Code session that launched this app is not this CLI's parent.
            .env_remove("CLAUDECODE")
            .env("CLAUDE_CODE_ENTRYPOINT", "sdk-forge-desktop")
            .envs(opts.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|e| format!("could not start {}: {e}", opts.binary.display()))?;
        let stdout = child.stdout.take().ok_or("the CLI has no stdout")?;
        let stderr = child.stderr.take().ok_or("the CLI has no stderr")?;
        let stdin = child.stdin.take().ok_or("the CLI has no stdin")?;
        log::write("info", &format!("session {} started (pid {}) in {}", opts.session_id, child.id(), opts.cwd.display()));

        let session = Arc::new(Session {
            id: opts.session_id.clone(),
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(Some(child)),
            pending: Mutex::new(HashMap::new()),
            next_request: AtomicU64::new(1),
            normalizer: Mutex::new(Normalizer::default()),
        });

        let tail = Arc::new(Mutex::new(Vec::<String>::new()));
        {
            let tail = tail.clone();
            let id = session.id.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    log::write("cli", &format!("{id}: {line}"));
                    let mut t = tail.lock().unwrap();
                    t.push(line);
                    if t.len() > 20 {
                        t.remove(0);
                    }
                }
            });
        }
        {
            let session = session.clone();
            let sink = sink.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    session.handle_line(&line, &sink);
                }
                let code = session.child.lock().unwrap().as_mut().and_then(|c| c.wait().ok()).and_then(|s| s.code());
                session.pending.lock().unwrap().clear();
                let stderr = tail.lock().unwrap().join("\n");
                log::write("info", &format!("session {} exited ({code:?})", session.id));
                session.emit(&sink, SessionEvent::Exited { code, stderr });
            });
        }
        session.request("initialize", json!({}))?;
        Ok(session)
    }

    fn emit(&self, sink: &Sink, event: SessionEvent) {
        sink(Envelope { session: self.id.clone(), event });
    }

    fn handle_line(&self, line: &str, sink: &Sink) {
        let Ok(Value::Object(event)) = serde_json::from_str::<Value>(line) else {
            log::write("cli-stdout", line);
            return;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("control_request") => self.on_control_request(&event, sink),
            Some("control_response") => {
                let response = event.get("response").cloned().unwrap_or(Value::Null);
                if response.get("subtype").and_then(Value::as_str) == Some("error") {
                    log::write("warn", &format!("{}: control request failed: {}", self.id, response));
                } else if let Some(r) = response.get("response").filter(|r| r.get("models").is_some() || r.get("commands").is_some()) {
                    self.emit(sink, SessionEvent::Capabilities {
                        models: r.get("models").cloned().unwrap_or(Value::Null),
                        commands: r.get("commands").cloned().unwrap_or(Value::Null),
                    });
                }
            }
            Some("control_cancel_request") => {
                if let Some(id) = event.get("request_id").and_then(Value::as_str) {
                    self.pending.lock().unwrap().remove(id);
                    self.emit(sink, SessionEvent::PermissionClosed { request_id: id.to_string(), reason: "cancelled".into() });
                }
            }
            Some(_) => {
                let events = self.normalizer.lock().unwrap().feed(&Value::Object(event));
                for e in events {
                    self.emit(sink, e);
                }
            }
            None => log::write("cli-stdout", line),
        }
    }

    fn on_control_request(&self, event: &Map<String, Value>, sink: &Sink) {
        let request_id = event.get("request_id").and_then(Value::as_str).unwrap_or_default().to_string();
        let request = event.get("request").cloned().unwrap_or(Value::Null);
        match request.get("subtype").and_then(Value::as_str) {
            Some("can_use_tool") => {
                let tool = request.get("tool_name").and_then(Value::as_str).unwrap_or_default().to_string();
                let input = request.get("input").cloned().unwrap_or(json!({}));
                let suggestions = request.get("permission_suggestions").cloned().unwrap_or(Value::Null);
                log::write("info", &format!("{}: asks to use {tool} ({request_id})", self.id));
                // Recorded before the window hears of it, so an instant answer finds it.
                self.pending.lock().unwrap().insert(request_id.clone(), Pending { tool: tool.clone(), input: input.clone(), suggestions: suggestions.clone() });
                self.emit(sink, SessionEvent::Permission {
                    request_id,
                    tool,
                    input,
                    description: request.get("description").and_then(Value::as_str).map(String::from),
                    tool_use_id: request.get("tool_use_id").and_then(Value::as_str).map(String::from),
                    can_always_allow: suggestions.as_array().is_some_and(|s| !s.is_empty()),
                });
            }
            other => {
                let error = format!("Forge Desktop does not handle {}", other.unwrap_or("this request"));
                let _ = self.write(&json!({"type": "control_response", "response": {"subtype": "error", "request_id": request_id, "error": error}}));
            }
        }
    }

    fn write(&self, value: &Value) -> Result<(), String> {
        let line = serde_json::to_string(value).map_err(|e| e.to_string())?;
        let mut guard = self.stdin.lock().unwrap();
        let stdin = guard.as_mut().ok_or("the session has ended")?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| e.to_string())
    }

    fn request(&self, subtype: &str, fields: Value) -> Result<(), String> {
        let id = format!("fd-{}", self.next_request.fetch_add(1, Ordering::Relaxed));
        let mut request = fields.as_object().cloned().unwrap_or_default();
        request.insert("subtype".into(), json!(subtype));
        self.write(&json!({"type": "control_request", "request_id": id, "request": request}))
    }

    pub fn send(&self, text: &str) -> Result<(), String> {
        if text.trim().is_empty() {
            return Err("empty message".into());
        }
        if text.len() > MAX_PROMPT_BYTES {
            return Err("message too long".into());
        }
        self.write(&json!({
            "type": "user",
            "message": {"role": "user", "content": text},
            "parent_tool_use_id": null,
            "session_id": self.id,
        }))
    }

    pub fn interrupt(&self) -> Result<(), String> {
        self.request("interrupt", json!({}))
    }

    pub fn set_mode(&self, mode: &str) -> Result<(), String> {
        if !MODES.contains(&mode) {
            return Err("unknown permission mode".into());
        }
        self.request("set_permission_mode", json!({"mode": mode}))
    }

    pub fn set_model(&self, model: &str) -> Result<(), String> {
        if !valid_model(model) {
            return Err("bad model name".into());
        }
        self.request("set_model", json!({"model": model}))
    }

    /// Answer a permission prompt the CLI is waiting on.
    pub fn respond(&self, request_id: &str, decision: Decision) -> Result<(), String> {
        let pending = self.pending.lock().unwrap().remove(request_id).ok_or("no such prompt is waiting")?;
        log::write("info", &format!("{}: {} answered: {}", self.id, pending.tool, match &decision {
            Decision::Allow => "allow",
            Decision::AllowAlways => "allow always",
            Decision::Deny(_) => "deny",
            Decision::Answer(_) => "answers",
        }));
        let response = match build_response(&pending, decision) {
            Ok(r) => r,
            Err(e) => {
                // A malformed answer leaves the prompt waiting.
                self.pending.lock().unwrap().insert(request_id.to_string(), pending);
                return Err(e);
            }
        };
        self.write(&json!({"type": "control_response", "response": {"subtype": "success", "request_id": request_id, "response": response}}))
    }

    pub fn is_running(&self) -> bool {
        self.stdin.lock().unwrap().is_some()
            && self.child.lock().unwrap().as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)))
    }

    /// Close stdin (the CLI's signal to finish), then make sure it is gone.
    pub fn stop(&self) {
        self.stdin.lock().unwrap().take();
        if let Some(mut child) = self.child.lock().unwrap().take() {
            for _ in 0..10 {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn build_response(pending: &Pending, decision: Decision) -> Result<Value, String> {
    Ok(match decision {
        Decision::Allow => json!({"behavior": "allow", "updatedInput": pending.input}),
        Decision::AllowAlways => {
            let mut r = json!({"behavior": "allow", "updatedInput": pending.input});
            if pending.suggestions.as_array().is_some_and(|s| !s.is_empty()) {
                r["updatedPermissions"] = pending.suggestions.clone();
            }
            r
        }
        Decision::Deny(message) => {
            let message: String = message.chars().take(2000).collect();
            let message = if message.trim().is_empty() { "The user declined this.".to_string() } else { message };
            json!({"behavior": "deny", "message": message})
        }
        Decision::Answer(answers) => {
            if pending.tool != "AskUserQuestion" {
                return Err("only a question takes answers".into());
            }
            let asked: Vec<&str> = pending
                .input
                .get("questions")
                .and_then(Value::as_array)
                .map(|qs| qs.iter().filter_map(|q| q.get("question").and_then(Value::as_str)).collect())
                .unwrap_or_default();
            for (question, answer) in &answers {
                if !asked.contains(&question.as_str()) {
                    return Err("an answer to a question that was not asked".into());
                }
                if !answer.as_str().is_some_and(|a| a.len() <= 4000) {
                    return Err("an answer must be text".into());
                }
            }
            let mut input = pending.input.clone();
            input["answers"] = Value::Object(answers);
            json!({"behavior": "allow", "updatedInput": input})
        }
    })
}

/// The folder Claude Code keeps a project's conversations in.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| home_dir().map(|h| h.join(".claude")))
}

pub fn projects_dir() -> Option<PathBuf> {
    config_dir().map(|c| c.join("projects"))
}

/// Claude Code's folder name for a project: every character that is not a
/// letter or digit becomes `-`.
pub fn project_slug(cwd: &Path) -> String {
    cwd.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

#[cfg(test)]
#[path = "cli_e2e.rs"]
mod e2e;

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(tool: &str, input: Value, suggestions: Value) -> Pending {
        Pending { tool: tool.into(), input, suggestions }
    }

    #[test]
    fn arguments_for_new_and_resumed_sessions() {
        let mut o = Options {
            binary: "claude".into(),
            cwd: ".".into(),
            session_id: "0e5b8a1c-1111-4222-8333-444455556666".into(),
            resume: false,
            model: Some("sonnet".into()),
            permission_mode: Some("plan".into()),
            env: vec![],
            env_remove: vec![],
        };
        let a = args(&o);
        assert!(a.windows(2).any(|w| w == ["--session-id", "0e5b8a1c-1111-4222-8333-444455556666"]));
        assert!(a.windows(2).any(|w| w == ["--permission-prompt-tool", "stdio"]));
        assert!(a.windows(2).any(|w| w == ["--model", "sonnet"]));
        assert!(a.windows(2).any(|w| w == ["--permission-mode", "plan"]));
        o.resume = true;
        assert!(args(&o).windows(2).any(|w| w == ["--resume", "0e5b8a1c-1111-4222-8333-444455556666"]));
    }

    #[test]
    fn ids_and_models_are_checked() {
        assert!(valid_session_id("0e5b8a1c-1111-4222-8333-444455556666"));
        assert!(!valid_session_id("../../etc"));
        assert!(!valid_session_id("0e5b8a1c-1111-4222-8333-44445555666"));
        assert!(valid_model("claude-sonnet-4-5[1m]"));
        assert!(valid_model("qwen3-coder:30b"));
        assert!(!valid_model("x; rm -rf /"));
        assert!(!valid_model(""));
    }

    #[test]
    fn allow_runs_the_input_the_cli_sent() {
        let p = pending("Write", json!({"file_path": "/a", "content": "x"}), json!([{"type": "setMode", "mode": "acceptEdits"}]));
        assert_eq!(build_response(&p, Decision::Allow).unwrap(), json!({"behavior": "allow", "updatedInput": {"file_path": "/a", "content": "x"}}));
        let always = build_response(&p, Decision::AllowAlways).unwrap();
        assert_eq!(always["updatedPermissions"], json!([{"type": "setMode", "mode": "acceptEdits"}]));
        let deny = build_response(&p, Decision::Deny(String::new())).unwrap();
        assert_eq!(deny["behavior"], "deny");
        assert!(!deny["message"].as_str().unwrap().is_empty());
    }

    #[test]
    fn answers_only_to_questions_that_were_asked() {
        let q = pending("AskUserQuestion", json!({"questions": [{"question": "Which?", "options": []}]}), Value::Null);
        let mut a = Map::new();
        a.insert("Which?".into(), json!("Left"));
        let r = build_response(&q, Decision::Answer(a)).unwrap();
        assert_eq!(r["updatedInput"]["answers"]["Which?"], "Left");
        assert_eq!(r["updatedInput"]["questions"][0]["question"], "Which?");

        let mut wrong = Map::new();
        wrong.insert("Something else?".into(), json!("x"));
        assert!(build_response(&q, Decision::Answer(wrong)).is_err());
        let w = pending("Write", json!({}), Value::Null);
        assert!(build_response(&w, Decision::Answer(Map::new())).is_err());
    }

    #[test]
    fn project_folder_names() {
        assert_eq!(project_slug(Path::new("/tmp/claude-0/-home/ws")), "-tmp-claude-0--home-ws");
        assert_eq!(project_slug(Path::new(r"C:\dev\my.app")), "C--dev-my-app");
    }
}
