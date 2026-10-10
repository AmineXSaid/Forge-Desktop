//! What the window sees of a session: one typed event stream, the same for a
//! live CLI and for a conversation loaded from disk.
//!
//! `Normalizer` turns the CLI's stream-json lines into `SessionEvent`s: text
//! and thinking arrive as deltas on a block key, tool calls by tool-use id,
//! results by the same id. `replay` turns a saved conversation into the same
//! events, so the UI has one reducer. `coalesce` merges consecutive deltas so
//! a fast stream costs one IPC message per frame, not one per token.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SessionEvent {
    /// The CLI is up for this session.
    Started { model: Option<String>, mode: Option<String> },
    /// What the person typed (saved conversations; live ones add it locally).
    User { text: String },
    TextDelta { block: String, text: String },
    ThinkingDelta { block: String, text: String },
    /// A finished text or thinking block that was not streamed.
    Text { block: String, text: String },
    Thinking { block: String, text: String },
    BlockDone { block: String },
    /// A tool call: first when it starts (`done: false`, input may be empty),
    /// again with its final input.
    Tool { id: String, name: String, input: Value, done: bool },
    ToolResult { id: String, output: String, is_error: bool },
    /// The CLI waits for an answer: allow, deny, or (AskUserQuestion) answers.
    Permission {
        request_id: String,
        tool: String,
        input: Value,
        description: Option<String>,
        tool_use_id: Option<String>,
        can_always_allow: bool,
    },
    PermissionClosed { request_id: String, reason: String },
    TurnEnd { ok: bool, duration_ms: Option<u64>, cost_usd: Option<f64>, error: Option<String> },
    Notice { tone: String, text: String },
    /// The CLI's answer to `initialize`: models and slash commands it offers.
    Capabilities {
        models: Value,
        commands: Value,
    },
    /// The process's lifecycle: "warming" (spawned), "ready", "running",
    /// "needsYou" (a prompt waits). A reaped session exits with `expected`.
    State { state: String },
    /// How full the context window is, after each turn.
    Context { used: u64, max: u64, percent: f64 },
    /// A title the CLI generated; it is also saved in the transcript.
    Title { title: String },
    /// Not one session's: the project's saved sessions changed on disk
    /// (sent with an empty session id).
    SessionsChanged,
    /// `expected`: Forge stopped it (closed, reaped, replaced), so it is no error.
    Exited { code: Option<i32>, stderr: String, expected: bool },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Envelope {
    pub session: String,
    pub event: SessionEvent,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Thinking,
}

#[derive(Default)]
pub struct Normalizer {
    message: String,
    streamed: HashSet<String>,
    blocks: HashMap<u64, (String, Kind)>,
    seq: u64,
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(String::from)
}

/// A tool result's text: a string, or the text of its blocks.
pub fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b.get("type").and_then(Value::as_str) {
                Some("text") => b.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
                Some("image") => "[image]".to_string(),
                _ => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn tool_results(content: &Value, out: &mut Vec<SessionEvent>) {
    for b in content.as_array().into_iter().flatten() {
        if b.get("type").and_then(Value::as_str) == Some("tool_result") {
            out.push(SessionEvent::ToolResult {
                id: str_of(b, "tool_use_id").unwrap_or_default(),
                output: result_text(b.get("content").unwrap_or(&Value::Null)),
                is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            });
        }
    }
}

impl Normalizer {
    fn next_key(&mut self, message: &str) -> String {
        self.seq += 1;
        format!("{message}:b{}", self.seq)
    }

    fn assistant_blocks(&mut self, message_id: &str, content: &Value, live: bool, out: &mut Vec<SessionEvent>) {
        let streamed = live && self.streamed.contains(message_id);
        for b in content.as_array().into_iter().flatten() {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_use") => out.push(SessionEvent::Tool {
                    id: str_of(b, "id").unwrap_or_default(),
                    name: str_of(b, "name").unwrap_or_default(),
                    input: b.get("input").cloned().unwrap_or(Value::Null),
                    done: true,
                }),
                Some("text") if !streamed => {
                    let text = str_of(b, "text").unwrap_or_default();
                    if !text.trim().is_empty() {
                        let block = self.next_key(message_id);
                        out.push(SessionEvent::Text { block, text });
                    }
                }
                Some("thinking") if !streamed => {
                    let text = str_of(b, "thinking").unwrap_or_default();
                    if !text.trim().is_empty() {
                        let block = self.next_key(message_id);
                        out.push(SessionEvent::Thinking { block, text });
                    }
                }
                _ => {}
            }
        }
    }

    /// One line from the CLI (already parsed), as zero or more events.
    pub fn feed(&mut self, e: &Value) -> Vec<SessionEvent> {
        let mut out = vec![];
        match e.get("type").and_then(Value::as_str) {
            Some("system") => match e.get("subtype").and_then(Value::as_str) {
                Some("init") => out.push(SessionEvent::Started { model: str_of(e, "model"), mode: str_of(e, "permissionMode") }),
                Some("compact_boundary") => out.push(SessionEvent::Notice { tone: "info".into(), text: "Earlier messages were summarized to free up context.".into() }),
                Some("api_retry") => out.push(SessionEvent::Notice {
                    tone: "warn".into(),
                    text: str_of(e, "message").unwrap_or_else(|| "The API is busy; retrying…".into()),
                }),
                _ => {}
            },
            Some("stream_event") => {
                let se = e.get("event").unwrap_or(&Value::Null);
                let index = se.get("index").and_then(Value::as_u64).unwrap_or(0);
                match se.get("type").and_then(Value::as_str) {
                    Some("message_start") => {
                        self.message = se.get("message").and_then(|m| str_of(m, "id")).unwrap_or_default();
                        self.streamed.insert(self.message.clone());
                        self.blocks.clear();
                    }
                    Some("content_block_start") => {
                        let b = se.get("content_block").unwrap_or(&Value::Null);
                        let message = self.message.clone();
                        match b.get("type").and_then(Value::as_str) {
                            Some("text") => {
                                let block = self.next_key(&message);
                                self.blocks.insert(index, (block.clone(), Kind::Text));
                                out.push(SessionEvent::TextDelta { block, text: str_of(b, "text").unwrap_or_default() });
                            }
                            Some("thinking") => {
                                let block = self.next_key(&message);
                                self.blocks.insert(index, (block.clone(), Kind::Thinking));
                                out.push(SessionEvent::ThinkingDelta { block, text: String::new() });
                            }
                            Some("tool_use") => out.push(SessionEvent::Tool {
                                id: str_of(b, "id").unwrap_or_default(),
                                name: str_of(b, "name").unwrap_or_default(),
                                input: Value::Object(Default::default()),
                                done: false,
                            }),
                            _ => {}
                        }
                    }
                    Some("content_block_delta") => {
                        let d = se.get("delta").unwrap_or(&Value::Null);
                        if let Some((block, kind)) = self.blocks.get(&index).cloned() {
                            match (d.get("type").and_then(Value::as_str), kind) {
                                (Some("text_delta"), Kind::Text) => out.push(SessionEvent::TextDelta { block, text: str_of(d, "text").unwrap_or_default() }),
                                (Some("thinking_delta"), Kind::Thinking) => {
                                    out.push(SessionEvent::ThinkingDelta { block, text: str_of(d, "thinking").unwrap_or_default() })
                                }
                                _ => {}
                            }
                        }
                    }
                    Some("content_block_stop") => {
                        if let Some((block, _)) = self.blocks.remove(&index) {
                            out.push(SessionEvent::BlockDone { block });
                        }
                    }
                    _ => {}
                }
            }
            Some("assistant") => {
                let m = e.get("message").unwrap_or(&Value::Null);
                let id = str_of(m, "id").unwrap_or_default();
                self.assistant_blocks(&id, m.get("content").unwrap_or(&Value::Null), true, &mut out);
            }
            Some("user") => tool_results(e.get("message").and_then(|m| m.get("content")).unwrap_or(&Value::Null), &mut out),
            Some("result") => {
                let ok = !e.get("is_error").and_then(Value::as_bool).unwrap_or(false) && e.get("subtype").and_then(Value::as_str).is_none_or(|s| s == "success");
                let error = (!ok).then(|| {
                    str_of(e, "result").filter(|r| !r.is_empty()).unwrap_or_else(|| match e.get("subtype").and_then(Value::as_str) {
                        Some("error_max_turns") => "Stopped: the turn limit was reached.".into(),
                        _ => "The turn ended with an error.".into(),
                    })
                });
                out.push(SessionEvent::TurnEnd {
                    ok,
                    duration_ms: e.get("duration_ms").and_then(Value::as_u64),
                    cost_usd: e.get("total_cost_usd").and_then(Value::as_f64),
                    error,
                });
            }
            _ => {}
        }
        out
    }
}

/// Text a person typed, from a saved user message; None for tool results,
/// command echoes and the CLI's own injected context.
pub fn typed_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            if blocks.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                return None;
            }
            blocks.iter().filter_map(|b| (b.get("type")?.as_str()? == "text").then(|| b.get("text")?.as_str()).flatten()).collect::<Vec<_>>().join("\n")
        }
        _ => return None,
    };
    let t = text.trim();
    if t.is_empty() || t.starts_with('<') || t.starts_with("Caveat:") {
        return None;
    }
    Some(t.to_string())
}

/// A saved conversation (`sessions::load`'s messages) as events.
pub fn replay(messages: &[Value]) -> Vec<SessionEvent> {
    let mut n = Normalizer::default();
    let mut out = vec![];
    for m in messages {
        let content = m.get("content").unwrap_or(&Value::Null);
        match m.get("role").and_then(Value::as_str) {
            Some("user") => match typed_text(content) {
                Some(text) => out.push(SessionEvent::User { text }),
                None => tool_results(content, &mut out),
            },
            Some("assistant") => {
                let id = str_of(m, "id").unwrap_or_else(|| "saved".into());
                n.assistant_blocks(&id, content, false, &mut out);
            }
            _ => {}
        }
    }
    out
}

/// Merge consecutive deltas on the same block, keeping every other event in order.
pub fn coalesce(batch: Vec<Envelope>) -> Vec<Envelope> {
    let mut out: Vec<Envelope> = Vec::with_capacity(batch.len());
    for e in batch {
        if let Some(last) = out.last_mut() {
            if last.session == e.session {
                match (&mut last.event, &e.event) {
                    (SessionEvent::TextDelta { block: a, text: t }, SessionEvent::TextDelta { block: b, text: u })
                    | (SessionEvent::ThinkingDelta { block: a, text: t }, SessionEvent::ThinkingDelta { block: b, text: u })
                        if a == b =>
                    {
                        t.push_str(u);
                        continue;
                    }
                    _ => {}
                }
            }
        }
        out.push(e);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn se(event: Value) -> Value {
        json!({"type": "stream_event", "event": event})
    }

    #[test]
    fn a_streamed_turn_with_a_tool_call() {
        let mut n = Normalizer::default();
        let mut all = vec![];
        for line in [
            json!({"type": "system", "subtype": "init", "model": "sonnet", "permissionMode": "default"}),
            se(json!({"type": "message_start", "message": {"id": "m1"}})),
            se(json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})),
            se(json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Writ"}})),
            se(json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "ing."}})),
            se(json!({"type": "content_block_stop", "index": 0})),
            se(json!({"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": "t1", "name": "Write", "input": {}}})),
            se(json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": "{}"}})),
            // The full message repeats the text (skipped: it was streamed) and gives the tool's input.
            json!({"type": "assistant", "message": {"id": "m1", "content": [{"type": "text", "text": "Writing."}]}}),
            json!({"type": "assistant", "message": {"id": "m1", "content": [{"type": "tool_use", "id": "t1", "name": "Write", "input": {"file_path": "a"}}]}}),
            json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": [{"type": "text", "text": "ok"}]}]}}),
            json!({"type": "result", "subtype": "success", "is_error": false, "duration_ms": 1200, "total_cost_usd": 0.01}),
        ] {
            all.extend(n.feed(&line));
        }
        let kinds: Vec<String> = all.iter().map(|e| serde_json::to_value(e).unwrap()["kind"].as_str().unwrap().to_string()).collect();
        assert_eq!(kinds, ["started", "textDelta", "textDelta", "textDelta", "blockDone", "tool", "tool", "toolResult", "turnEnd"]);
        assert_eq!(all[5], SessionEvent::Tool { id: "t1".into(), name: "Write".into(), input: json!({}), done: false });
        assert_eq!(all[6], SessionEvent::Tool { id: "t1".into(), name: "Write".into(), input: json!({"file_path": "a"}), done: true });
        assert_eq!(all[7], SessionEvent::ToolResult { id: "t1".into(), output: "ok".into(), is_error: false });
        assert_eq!(all[8], SessionEvent::TurnEnd { ok: true, duration_ms: Some(1200), cost_usd: Some(0.01), error: None });
    }

    #[test]
    fn errors_end_the_turn_with_a_message() {
        let mut n = Normalizer::default();
        let e = n.feed(&json!({"type": "result", "subtype": "error_max_turns", "is_error": true}));
        assert_eq!(e, [SessionEvent::TurnEnd { ok: false, duration_ms: None, cost_usd: None, error: Some("Stopped: the turn limit was reached.".into()) }]);
    }

    #[test]
    fn saved_conversations_replay_as_the_same_events() {
        let msgs = vec![
            json!({"role": "user", "content": "Fix it"}),
            json!({"role": "user", "content": "<local-command-stdout>x</local-command-stdout>"}),
            json!({"role": "assistant", "id": "m1", "content": [{"type": "thinking", "thinking": "hmm"}, {"type": "text", "text": "On it."}]}),
            json!({"role": "assistant", "id": "m1", "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}),
            json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "body", "is_error": true}]}),
        ];
        let e = replay(&msgs);
        assert_eq!(e[0], SessionEvent::User { text: "Fix it".into() });
        assert!(matches!(&e[1], SessionEvent::Thinking { text, .. } if text == "hmm"));
        assert!(matches!(&e[2], SessionEvent::Text { text, .. } if text == "On it."));
        assert!(matches!(&e[3], SessionEvent::Tool { done: true, .. }));
        assert_eq!(e[4], SessionEvent::ToolResult { id: "t1".into(), output: "body".into(), is_error: true });
        assert_eq!(e.len(), 5);
    }

    #[test]
    fn deltas_merge_per_block_and_order_is_kept() {
        let env = |s: &str, e: SessionEvent| Envelope { session: s.into(), event: e };
        let d = |b: &str, t: &str| SessionEvent::TextDelta { block: b.into(), text: t.into() };
        let out = coalesce(vec![
            env("a", d("x", "He")),
            env("a", d("x", "llo")),
            env("b", d("x", "!")),
            env("a", d("x", " there")),
            env("a", SessionEvent::BlockDone { block: "x".into() }),
            env("a", d("y", "1")),
            env("a", d("y", "2")),
        ]);
        assert_eq!(out.len(), 5);
        assert_eq!(out[0].event, d("x", "Hello"));
        assert_eq!(out[2].event, d("x", " there"));
        assert_eq!(out[4].event, d("y", "12"));
    }
}
