//! End to end against the real Claude Code CLI and a stub Anthropic API
//! (`test/stub-anthropic.mjs`): a Write is prompted, allowed, lands on disk,
//! the conversation is saved where the sidebar reads it, and a second process
//! resumes it. Skipped when `claude` or `node` is not on this machine.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;


use super::*;
use crate::events::{Envelope, SessionEvent};
use crate::sessions;

struct Stub(std::process::Child);
impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn start_stub() -> Option<(Stub, u16)> {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("test").join("stub-anthropic.mjs");
    let mut child = Command::new("node").arg(script).stdout(Stdio::piped()).spawn().ok()?;
    let mut line = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
    let port = line.trim().strip_prefix("listening ")?.parse().ok()?;
    Some((Stub(child), port))
}

fn wait_for(rx: &mpsc::Receiver<Envelope>, what: &str, pred: impl Fn(&SessionEvent) -> bool) -> SessionEvent {
    loop {
        match rx.recv_timeout(Duration::from_secs(90)) {
            Ok(e) if pred(&e.event) => return e.event,
            Ok(Envelope { event: e @ SessionEvent::Exited { .. }, .. }) => panic!("the CLI exited while waiting for {what}: {e:?}"),
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }
}

fn is_turn_end(e: &SessionEvent) -> bool {
    matches!(e, SessionEvent::TurnEnd { ok: true, .. })
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

    const FIRST: &str = "6f1d3c2a-0b9e-4c11-8a7d-2e5f9b0c1d42";
    const FORK: &str = "7a2e4d3b-1c0f-4d22-9b8e-3f6a0c1d2e53";
    let options = |resume: bool| Options {
        binary: binary.clone(),
        cwd: ws.clone(),
        session_id: FIRST.into(),
        resume,
        fork_from: None,
        thinking: None,
        want_title: !resume,
        model: Some("claude-sonnet-4-5".into()),
        permission_mode: Some("default".into()),
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
    let (tx, rx) = mpsc::channel::<Envelope>();
    let sink: Sink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let s = Session::start(options(false), sink).unwrap();
    s.send(&format!("write hello in {}", ws.display())).unwrap();
    // With nonessential traffic off the CLI would default to `auto`; the mode we pass wins.
    let started = wait_for(&rx, "the CLI to start", |e| matches!(e, SessionEvent::Started { .. }));
    assert_eq!(started, SessionEvent::Started { model: Some("claude-sonnet-4-5".into()), mode: Some("default".into()) });
    let SessionEvent::Permission { request_id, tool, .. } = wait_for(&rx, "the permission prompt", |e| matches!(e, SessionEvent::Permission { .. })) else { unreachable!() };
    assert_eq!(tool, "Write");
    assert!(!ws.join("hello.txt").exists(), "nothing is written before the answer");
    assert!(s.respond("not-a-waiting-prompt", Decision::Allow).is_err());
    s.respond(&request_id, Decision::Allow).unwrap();
    wait_for(&rx, "the file's tool result", |e| matches!(e, SessionEvent::ToolResult { is_error: false, .. }));
    wait_for(&rx, "the end of the turn", is_turn_end);
    assert_eq!(std::fs::read_to_string(ws.join("hello.txt")).unwrap(), "hello\n");
    // After the turn: how full the context is, and a title the CLI saves.
    let SessionEvent::Context { used, max, .. } = wait_for(&rx, "the context usage", |e| matches!(e, SessionEvent::Context { .. })) else { unreachable!() };
    assert!(used > 0 && max >= used, "{used} of {max}");
    let SessionEvent::Title { title } = wait_for(&rx, "the generated title", |e| matches!(e, SessionEvent::Title { .. })) else { unreachable!() };
    assert!(title.starts_with("Stub: write hello in"), "{title}");
    assert!(!s.is_busy() && !s.needs_you());
    s.stop();
    let exited = wait_for(&rx, "the exit", |e| matches!(e, SessionEvent::Exited { .. }));
    assert!(matches!(exited, SessionEvent::Exited { expected: true, .. }), "stopping is no error: {exited:?}");

    // 2. Saved where the sidebar looks, with the first message as its title.
    let file = config.join("projects").join(project_slug(&ws)).join("6f1d3c2a-0b9e-4c11-8a7d-2e5f9b0c1d42.jsonl");
    let text = std::fs::read_to_string(&file).expect("the conversation is saved under the project's folder");
    let summary = sessions::summarize(FIRST, &text, std::time::SystemTime::now()).unwrap();
    assert!(summary.title.starts_with("Stub: write hello in"), "the generated title names it: {}", summary.title);
    let before = sessions::load(&file).unwrap().len();
    assert!(before >= 4, "user, assistant text, tool use, tool result: {before}");

    // 3. A second process resumes the same conversation.
    let (tx, rx) = mpsc::channel::<Envelope>();
    let sink: Sink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let s = Session::start(Options { thinking: Some(crate::cli::Thinking::Low), ..options(true) }, sink).unwrap();
    s.send("hi again").unwrap();
    wait_for(&rx, "the resumed CLI", |e| matches!(e, SessionEvent::Started { .. }));
    wait_for(&rx, "the resumed reply", |e| matches!(e, SessionEvent::TextDelta { text, .. } if text.contains("Hello from the stub")));
    wait_for(&rx, "the resumed turn", is_turn_end);
    s.stop();
    let after = sessions::load(&file).unwrap();
    assert!(after.len() > before);
    assert!(after.iter().any(|m| m["content"] == "hi again"));
    let replayed = crate::events::replay(&after);
    assert!(matches!(&replayed[0], SessionEvent::User { text } if text.starts_with("write hello in")));
    assert!(replayed.iter().any(|e| matches!(e, SessionEvent::Tool { name, done: true, .. } if name == "Write")));

    // 4. A fork: a new id that starts from a copy of the conversation; the original is untouched.
    let (tx, rx) = mpsc::channel::<Envelope>();
    let sink: Sink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let s = Session::start(Options { session_id: FORK.into(), fork_from: Some(FIRST.into()), want_title: false, ..options(false) }, sink).unwrap();
    s.send("on the fork").unwrap();
    wait_for(&rx, "the fork's turn", is_turn_end);
    // 5. Reaping: an idle session's process is stopped, and that is no error.
    let manager = crate::session_manager::Manager::new(crate::session_manager::Limits { idle: std::time::Duration::ZERO, max_live: 6 });
    manager.insert(s.clone());
    assert!(manager.get(FORK).is_none(), "an idle session past the limit is reaped");
    let exited = wait_for(&rx, "the reaped exit", |e| matches!(e, SessionEvent::Exited { .. }));
    assert!(matches!(exited, SessionEvent::Exited { expected: true, .. }), "{exited:?}");
    let forked = sessions::load(&file.with_file_name(format!("{FORK}.jsonl"))).expect("the fork is saved under its own id");
    assert!(forked.iter().any(|m| m["content"] == "hi again"), "the fork has the history");
    assert!(forked.iter().any(|m| m["content"] == "on the fork"));
    assert!(!sessions::load(&file).unwrap().iter().any(|m| m["content"] == "on the fork"), "the original is untouched");
    let _ = std::fs::remove_dir_all(&root);
}
