//! The real Claude Code CLI through the gateway, against stub servers: the
//! `openai-chat` wire (a tool-calling round trip, a streamed thinking turn, a
//! 429 the CLI retries, a generated title) and the `anthropic` passthrough.
//! The stubs accept only the key Forge holds, so the key provably comes from
//! the gateway, not the CLI. Skipped without `claude` or `node`.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use super::*;
use crate::cli::{find_claude, Decision, Options, Session, Sink};
use crate::events::{Envelope, SessionEvent};

struct Stub(Child);
impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn stub(script: &str, key: &str) -> Option<(Stub, u16)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(script);
    let trace = std::env::var_os("FORGE_E2E_TRACE").is_some();
    let mut cmd = Command::new("node");
    cmd.arg(path).env("STUB_KEY", key).stdout(Stdio::piped()).stderr(if trace { Stdio::inherit() } else { Stdio::null() });
    if trace {
        cmd.env("STUB_LOG", "1");
    }
    let mut child = cmd.spawn().ok()?;
    let mut line = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
    let port = line.trim().strip_prefix("listening ")?.parse().ok()?;
    Some((Stub(child), port))
}

fn wait_for(rx: &mpsc::Receiver<Envelope>, what: &str, pred: impl Fn(&SessionEvent) -> bool) -> SessionEvent {
    loop {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(Envelope { event, .. }) if pred(&event) => return event,
            Ok(Envelope { event, .. }) if std::env::var_os("FORGE_E2E_TRACE").is_some() => eprintln!("event: {event:?}"),
            Ok(Envelope { event: SessionEvent::TurnEnd { ok: false, error, .. }, .. }) => panic!("the turn failed while waiting for {what}: {error:?}"),
            Ok(Envelope { event: e @ SessionEvent::Exited { .. }, .. }) => panic!("the CLI exited while waiting for {what}: {e:?}"),
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }
}

fn turn_end(e: &SessionEvent) -> bool {
    matches!(e, SessionEvent::TurnEnd { .. })
}

struct Setup {
    root: std::path::PathBuf,
    ws: std::path::PathBuf,
    base_env: Vec<(String, String)>,
}

fn setup(name: &str) -> Setup {
    let root = std::env::temp_dir().join(format!("forge-gw-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (home, ws) = (root.join("home"), root.join("ws"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&ws).unwrap();
    let ws = ws.canonicalize().unwrap();
    let base_env = vec![
        ("HOME".into(), home.display().to_string()),
        ("USERPROFILE".into(), home.display().to_string()),
        ("CLAUDE_CONFIG_DIR".into(), home.join(".claude").display().to_string()),
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(), "1".into()),
    ];
    Setup { root, ws, base_env }
}

fn options(s: &Setup, binary: &std::path::Path, id: &str, gateway_env: Vec<(String, String)>) -> Options {
    let mut env = s.base_env.clone();
    env.extend(gateway_env);
    Options {
        binary: binary.to_path_buf(),
        cwd: s.ws.clone(),
        session_id: id.into(),
        resume: false,
        fork_from: None,
        model: None,
        permission_mode: Some("default".into()),
        env,
        env_remove: std::env::vars().map(|(k, _)| k).filter(|k| k.starts_with("ANTHROPIC") || k.starts_with("CLAUDE")).collect(),
        thinking: None,
        want_title: true,
    }
}

fn channel() -> (Sink, mpsc::Receiver<Envelope>) {
    let (tx, rx) = mpsc::channel::<Envelope>();
    (Arc::new(move |e| {
        let _ = tx.send(e);
    }), rx)
}

#[test]
fn the_cli_works_through_the_openai_chat_wire() {
    let Some(binary) = find_claude() else { return eprintln!("skipped: no claude CLI") };
    let Some((_stub, port)) = stub("stub-openai.mjs", "sk-openai-stub") else { return eprintln!("skipped: no node for the stub API") };
    let resolve: Resolve = Arc::new(move |p: &str| {
        (p == "openai").then(|| Upstream {
            wire: Wire::OpenAiChat,
            base_url: format!("http://127.0.0.1:{port}/v1"),
            auth: Auth::Bearer,
            key: Some("sk-openai-stub".into()),
            chat: openai_chat::Options { max_tokens_field: "max_completion_tokens".into(), reasoning: true },
        })
    });
    let gw = Gateway::start(resolve).unwrap();
    let s = setup("chat");
    let (sink, rx) = channel();
    let session = Session::start(options(&s, &binary, "8b3f5e4c-2d1a-4e33-a09f-4a7b1d2e3f64", gw.env("openai", "gpt-stub", "gpt-stub-fast")), sink).unwrap();

    // A tool call through the translation: asked, allowed, written.
    session.send(&format!("write hello in {}", s.ws.display())).unwrap();
    let started = wait_for(&rx, "the CLI to start", |e| matches!(e, SessionEvent::Started { .. }));
    assert_eq!(started, SessionEvent::Started { model: Some("openai/gpt-stub".into()), mode: Some("default".into()) });
    wait_for(&rx, "the streamed thinking", |e| matches!(e, SessionEvent::ThinkingDelta { text, .. } if text.contains("wants a file")));
    let SessionEvent::Permission { request_id, tool, input, .. } = wait_for(&rx, "the permission prompt", |e| matches!(e, SessionEvent::Permission { .. })) else { unreachable!() };
    assert_eq!(tool, "Write");
    assert_eq!(input["content"], "hello\n", "arguments streamed in pieces arrive whole");
    session.respond(&request_id, Decision::Allow).unwrap();
    wait_for(&rx, "the reply after the tool", |e| matches!(e, SessionEvent::TextDelta { text, .. } if text.contains("Done.")));
    wait_for(&rx, "the end of the turn", turn_end);
    assert_eq!(std::fs::read_to_string(s.ws.join("hello.txt")).unwrap(), "hello\n");
    let SessionEvent::Title { title } = wait_for(&rx, "the title (structured output)", |e| matches!(e, SessionEvent::Title { .. })) else { unreachable!() };
    assert!(title.starts_with("OpenAI: write hello in"), "{title}");

    // Reasoning becomes a thinking block.
    session.send("think about it").unwrap();
    wait_for(&rx, "the thinking turn", |e| matches!(e, SessionEvent::ThinkingDelta { text, .. } if text.contains("Weighing")));
    wait_for(&rx, "the answer", |e| matches!(e, SessionEvent::TextDelta { text, .. } if text.contains("Thought it through")));
    wait_for(&rx, "the end of the thinking turn", turn_end);

    // A 429 comes back as rate_limit_error, which the CLI retries by itself.
    session.send("rate limit once").unwrap();
    wait_for(&rx, "the answer after the retry", |e| matches!(e, SessionEvent::TextDelta { text, .. } if text.contains("Hello from the OpenAI stub")));
    wait_for(&rx, "the end of the retried turn", turn_end);
    session.stop();

    // A wrong token is refused at the door.
    let wrong = reqwest::blocking_free_post(gw.port, "Bearer nope.openai");
    assert_eq!(wrong, 401);
    let _ = std::fs::remove_dir_all(&s.root);
}

#[test]
fn the_cli_works_through_the_anthropic_passthrough() {
    let Some(binary) = find_claude() else { return eprintln!("skipped: no claude CLI") };
    let Some((_stub, port)) = stub("stub-anthropic.mjs", "sk-ant-stub") else { return eprintln!("skipped: no node for the stub API") };
    let resolve: Resolve = Arc::new(move |p: &str| {
        (p == "anth").then(|| Upstream { wire: Wire::Anthropic, base_url: format!("http://127.0.0.1:{port}"), auth: Auth::XApiKey, key: Some("sk-ant-stub".into()), chat: Default::default() })
    });
    let gw = Gateway::start(resolve).unwrap();
    let s = setup("anth");
    let (sink, rx) = channel();
    let session = Session::start(options(&s, &binary, "9c4a6f5d-3e2b-4f44-b1a0-5b8c2e3f4a75", gw.env("anth", "claude-stub", "")), sink).unwrap();
    session.send(&format!("write hello in {}", s.ws.display())).unwrap();
    let SessionEvent::Permission { request_id, .. } = wait_for(&rx, "the permission prompt", |e| matches!(e, SessionEvent::Permission { .. })) else { unreachable!() };
    session.respond(&request_id, Decision::Allow).unwrap();
    wait_for(&rx, "the end of the turn", turn_end);
    assert_eq!(std::fs::read_to_string(s.ws.join("hello.txt")).unwrap(), "hello\n");
    let SessionEvent::Title { title } = wait_for(&rx, "the title", |e| matches!(e, SessionEvent::Title { .. })) else { unreachable!() };
    assert!(title.starts_with("Stub: write hello in"), "{title}");
    session.stop();
    let _ = std::fs::remove_dir_all(&s.root);
}

mod reqwest {
    /// POST /v1/messages with an Authorization header; the status code.
    pub fn blocking_free_post(port: u16, auth: &str) -> u16 {
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let body = "{}";
        write!(s, "POST /v1/messages HTTP/1.1\r\nHost: x\r\nAuthorization: {auth}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0)
    }
}
