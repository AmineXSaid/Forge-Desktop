//! The Node sidecar: Forge's host, one process per window (plan §4.3).
//!
//! stdin carries frames to Node and stdout frames back, one JSON object per
//! line (`host/runtime.ts` documents them). A stdout line that is not a
//! `$forge` frame is logged, never parsed into the page. stderr is logged.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::{host_calls, paths};

/// What the window and the host share.
#[derive(Default)]
pub struct Bridge {
    inner: Mutex<BridgeState>,
}

#[derive(Default)]
struct BridgeState {
    stdin: Option<ChildStdin>,
    child: Option<Child>,
    /// Page messages sent before the host was up, in order.
    inbound: Vec<String>,
    /// The page's delivery channel, once it attached.
    channel: Option<Channel<Value>>,
    /// Host messages sent before the page attached, in order.
    outbound: Vec<Value>,
    /// Bumped by every reset, so a host that was replaced on purpose (a
    /// project switch) is not reported as having crashed.
    generation: u64,
}

impl Bridge {
    /// A page message for the host: written now, or queued until it starts.
    pub fn to_host(&self, message: Value) -> Result<(), String> {
        let line = serde_json::to_string(&json!({"$forge": "in", "message": message})).map_err(|e| e.to_string())?;
        let mut state = self.inner.lock().unwrap();
        match state.stdin.as_mut() {
            Some(stdin) => write_line(stdin, &line),
            None => {
                state.inbound.push(line);
                Ok(())
            }
        }
    }

    /// A frame of our own for the host (host-call answers, visibility).
    pub fn frame_to_host(&self, frame: Value) {
        let Ok(line) = serde_json::to_string(&frame) else { return };
        let mut state = self.inner.lock().unwrap();
        if let Some(stdin) = state.stdin.as_mut() {
            if let Err(e) = write_line(stdin, &line) {
                paths::log("warn", &format!("could not write to the host: {e}"));
            }
        }
    }

    /// The page attached its channel: deliver what waited.
    pub fn attach(&self, channel: Channel<Value>) {
        let mut state = self.inner.lock().unwrap();
        for message in state.outbound.drain(..) {
            let _ = channel.send(message);
        }
        state.channel = Some(channel);
    }

    fn to_page(&self, message: Value) {
        let mut state = self.inner.lock().unwrap();
        match state.channel.as_ref() {
            Some(channel) => {
                if channel.send(message.clone()).is_err() {
                    // The page reloaded: hold messages for its next attach.
                    state.channel = None;
                    state.outbound.push(message);
                }
            }
            None => state.outbound.push(message),
        }
    }

    /// Stop the host and forget the page's channel and queues: the next
    /// `start` begins clean (a project switch, followed by a page reload).
    pub fn reset(&self) {
        self.stop();
        let mut state = self.inner.lock().unwrap();
        state.generation += 1;
        state.channel = None;
        state.inbound.clear();
        state.outbound.clear();
    }

    fn generation(&self) -> u64 {
        self.inner.lock().unwrap().generation
    }

    pub fn stop(&self) {
        let mut state = self.inner.lock().unwrap();
        state.stdin = None; // closing stdin is the host's signal to exit
        if let Some(mut child) = state.child.take() {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn write_line(stdin: &mut ChildStdin, line: &str) -> Result<(), String> {
    stdin.write_all(line.as_bytes()).and_then(|_| stdin.write_all(b"\n")).and_then(|_| stdin.flush()).map_err(|e| e.to_string())
}

/// Answer a host call.
pub fn answer(app: &AppHandle, id: u64, result: Result<Value, String>) {
    let frame = match result {
        Ok(value) => json!({"$forge": "return", "id": id, "result": value}),
        Err(error) => json!({"$forge": "return", "id": id, "error": error}),
    };
    app.state::<Bridge>().frame_to_host(frame);
}

/// What one stdout line is.
#[derive(Debug, PartialEq)]
pub enum Frame {
    Ready,
    Out(Value),
    Call { id: u64, method: String, params: Value },
    /// Not a frame: a stray print, logged.
    Noise(String),
}

pub fn parse_frame(line: &str) -> Frame {
    let Ok(value) = serde_json::from_str::<Value>(line) else { return Frame::Noise(line.to_string()) };
    match value.get("$forge").and_then(Value::as_str) {
        Some("ready") => Frame::Ready,
        Some("out") => match value.get("message") {
            // Only what Forge's WebViewService posts: the from-extension wrapper.
            Some(m) if m.get("type").and_then(Value::as_str) == Some("from-extension") => Frame::Out(m.clone()),
            _ => Frame::Noise(line.to_string()),
        },
        Some("call") => match (value.get("id").and_then(Value::as_u64), value.get("method").and_then(Value::as_str)) {
            (Some(id), Some(method)) => Frame::Call { id, method: method.to_string(), params: value.get("params").cloned().unwrap_or(Value::Null) },
            _ => Frame::Noise(line.to_string()),
        },
        _ => Frame::Noise(line.to_string()),
    }
}

/// `node`: `$FORGE_NODE`, else the first `node` on PATH.
pub fn find_node() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FORGE_NODE").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let names: &[&str] = if cfg!(windows) { &["node.exe"] } else { &["node"] };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

pub struct Launch<'a> {
    pub node: &'a Path,
    pub resources: &'a Path,
    pub workspace: &'a Path,
    pub data_dir: &'a Path,
    pub log_dir: &'a Path,
    pub version: String,
}

/// Start the host and wire its output to the page.
pub fn start(app: &AppHandle, launch: Launch) -> Result<(), String> {
    let script = launch.resources.join("dist").join("forge-host.cjs");
    if !script.is_file() {
        return Err(format!("Forge's host is missing: {}", script.display()));
    }
    let mut command = Command::new(launch.node);
    command
        .arg(&script)
        .current_dir(launch.workspace)
        .env("FORGE_WORKSPACE", launch.workspace)
        .env("FORGE_RESOURCES", launch.resources)
        .env("FORGE_DATA_DIR", launch.data_dir)
        .env("FORGE_LOG_DIR", launch.log_dir)
        .env("FORGE_APP_VERSION", &launch.version)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| format!("could not start {}: {e}", launch.node.display()))?;
    let stdout = child.stdout.take().ok_or("the host has no stdout")?;
    let stderr = child.stderr.take().ok_or("the host has no stderr")?;
    let mut stdin = child.stdin.take().ok_or("the host has no stdin")?;
    paths::log("info", &format!("host started (pid {}) in {}", child.id(), launch.workspace.display()));

    let generation;
    {
        let bridge = app.state::<Bridge>();
        let mut state = bridge.inner.lock().unwrap();
        generation = state.generation;
        for line in state.inbound.drain(..) {
            write_line(&mut stdin, &line)?;
        }
        state.stdin = Some(stdin);
        state.child = Some(child);
    }

    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            paths::log("host", &line);
        }
    });

    let app = app.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match parse_frame(&line) {
                Frame::Ready => paths::log("info", "host ready"),
                Frame::Out(message) => app.state::<Bridge>().to_page(message),
                Frame::Call { id, method, params } => {
                    let app = app.clone();
                    // Dialogs answer from their own callbacks; the reader keeps reading.
                    let _ = app.clone().run_on_main_thread(move || host_calls::handle(&app, id, &method, &params));
                }
                Frame::Noise(text) => paths::log("host-stdout", &text),
            }
        }
        paths::log("warn", "the host's output closed");
        if app.state::<Bridge>().generation() == generation {
            crate::host_exited(&app);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_told_apart_from_noise() {
        assert_eq!(parse_frame(r#"{"$forge":"ready"}"#), Frame::Ready);
        let out = r#"{"$forge":"out","message":{"type":"from-extension","message":{"type":"response"}}}"#;
        assert!(matches!(parse_frame(out), Frame::Out(_)));
        let call = r#"{"$forge":"call","id":3,"method":"app.info","params":{}}"#;
        assert_eq!(parse_frame(call), Frame::Call { id: 3, method: "app.info".into(), params: json!({}) });
        assert!(matches!(parse_frame("Debugger attached."), Frame::Noise(_)));
        assert!(matches!(parse_frame(r#"{"type":"from-extension"}"#), Frame::Noise(_)));
        // An "out" that is not the from-extension wrapper never reaches the page.
        assert!(matches!(parse_frame(r#"{"$forge":"out","message":{"type":"exec"}}"#), Frame::Noise(_)));
        assert!(matches!(parse_frame(r#"{"$forge":"call","method":"app.info"}"#), Frame::Noise(_)));
    }
}
