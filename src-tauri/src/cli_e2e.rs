//! End to end against the real Claude Code CLI and a stub Anthropic API
//! (`test/stub-anthropic.mjs`): a Write is prompted, allowed, lands on disk,
//! the conversation is saved where the sidebar reads it, and a second process
//! resumes it. Skipped when `claude` or `node` is not on this machine.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

use super::*;
use crate::sessions;

struct Stub(std::process::Child);
impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn start_stub() -> Option<(Stub, u16)> {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("test").join("stub-anthropic.mjs");
    let mut child = Command::new("node").arg(script).stdout(Stdio::piped()).spawn().ok()?;
    let mut line = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
    let port = line.trim().strip_prefix("listening ")?.parse().ok()?;
    Some((Stub(child), port))
}

fn wait_for(rx: &mpsc::Receiver<Value>, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
    loop {
        match rx.recv_timeout(Duration::from_secs(90)) {
            Ok(e) if pred(&e) => return e,
            Ok(e) if e["type"] == "exit" => panic!("the CLI exited while waiting for {what}: {e}"),
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }
}

fn is_result(e: &Value) -> bool {
    e["type"] == "cli" && e["event"]["type"] == "result"
}

#[test]
fn prompt_allow_write_save_and_resume() {
    let Some(binary) = find_claude() else { return eprintln!("skipped: no claude CLI") };
    let Some((_stub, port)) = start_stub() else { return eprintln!("skipped: no node for the stub API") };

    let root = std::env::temp_dir().join(format!("forge-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (home, ws) = (root.join("home"), root.join("ws"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&ws).unwrap();
    let ws = ws.canonicalize().unwrap();
    let config = home.join(".claude");

    let options = |resume: bool| Options {
        binary: binary.clone(),
        cwd: ws.clone(),
        session_id: "6f1d3c2a-0b9e-4c11-8a7d-2e5f9b0c1d42".into(),
        resume,
        model: Some("claude-sonnet-4-5".into()),
        permission_mode: None,
        env: vec![
            ("HOME".into(), home.display().to_string()),
            ("USERPROFILE".into(), home.display().to_string()),
            ("CLAUDE_CONFIG_DIR".into(), config.display().to_string()),
            ("ANTHROPIC_BASE_URL".into(), format!("http://127.0.0.1:{port}")),
            ("ANTHROPIC_API_KEY".into(), "stub".into()),
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(), "1".into()),
        ],
        // A run inside a Claude Code session must not hand its own setup to the CLI under test.
        env_remove: std::env::vars().map(|(k, _)| k).filter(|k| k.starts_with("ANTHROPIC") || k.starts_with("CLAUDE")).collect(),
    };

    // 1. A new session asks before writing; allowing it writes the file.
    let (tx, rx) = mpsc::channel::<Value>();
    let sink: Sink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let s = Session::start(options(false), sink).unwrap();
    s.send(&format!("write hello in {}", ws.display())).unwrap();
    let prompt = wait_for(&rx, "the permission prompt", |e| e["type"] == "permission");
    assert_eq!(prompt["tool"], "Write");
    assert!(!ws.join("hello.txt").exists(), "nothing is written before the answer");
    assert!(s.respond("not-a-waiting-prompt", Decision::Allow).is_err());
    s.respond(prompt["requestId"].as_str().unwrap(), Decision::Allow).unwrap();
    wait_for(&rx, "the end of the turn", is_result);
    assert_eq!(std::fs::read_to_string(ws.join("hello.txt")).unwrap(), "hello\n");
    s.stop();

    // 2. Saved where the sidebar looks, with the first message as its title.
    let file = config.join("projects").join(project_slug(&ws)).join("6f1d3c2a-0b9e-4c11-8a7d-2e5f9b0c1d42.jsonl");
    let text = std::fs::read_to_string(&file).expect("the conversation is saved under the project's folder");
    let summary = sessions::summarize("6f1d3c2a-0b9e-4c11-8a7d-2e5f9b0c1d42", &text, std::time::SystemTime::now()).unwrap();
    assert!(summary.title.starts_with("write hello in"));
    let before = sessions::load(&file).unwrap().len();
    assert!(before >= 4, "user, assistant text, tool use, tool result: {before}");

    // 3. A second process resumes the same conversation.
    let (tx, rx) = mpsc::channel::<Value>();
    let sink: Sink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let s = Session::start(options(true), sink).unwrap();
    s.send("hi again").unwrap();
    let init = wait_for(&rx, "init", |e| e["type"] == "cli" && e["event"]["subtype"] == "init");
    assert_eq!(init["event"]["session_id"], "6f1d3c2a-0b9e-4c11-8a7d-2e5f9b0c1d42");
    wait_for(&rx, "the resumed turn", is_result);
    s.stop();
    let after = sessions::load(&file).unwrap();
    assert!(after.len() > before);
    assert!(after.iter().any(|m| m["content"] == "hi again"));
    let _ = std::fs::remove_dir_all(&root);
}
