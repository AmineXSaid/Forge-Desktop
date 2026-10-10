//! The `openai-chat` wire: Anthropic Messages in, OpenAI chat completions out,
//! and the reply back as Anthropic events, streamed.
//!
//! Request: system prompt, text and image blocks (images as data URLs),
//! `tool_use` -> `tool_calls`, `tool_result` -> `role: tool`, tool schemas
//! and `tool_choice`, stop sequences, and a thinking budget -> a reasoning
//! effort where the model has one. Earlier thinking blocks are dropped: they
//! carry signatures only Anthropic can check.
//!
//! Reply: chunks become `message_start`, `content_block_*` (text, thinking,
//! tool_use with `input_json_delta`), `message_delta` with the stop reason
//! and usage, and `message_stop`.

use serde_json::{json, Map, Value};

/// How one provider takes a chat request.
#[derive(Debug, Clone)]
pub struct Options {
    /// "max_tokens", or "max_completion_tokens" (OpenAI's reasoning models).
    pub max_tokens_field: String,
    /// Send `reasoning_effort` when the request asks for thinking.
    pub reasoning: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { max_tokens_field: "max_tokens".into(), reasoning: false }
    }
}

fn text_of_blocks(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| match b.get("type").and_then(Value::as_str) {
                Some("text") => b.get("text").and_then(Value::as_str).map(String::from),
                Some("image") => Some("[image]".into()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn image_part(b: &Value) -> Option<Value> {
    let src = b.get("source")?;
    let url = match src.get("type").and_then(Value::as_str) {
        Some("base64") => format!("data:{};base64,{}", src.get("media_type")?.as_str()?, src.get("data")?.as_str()?),
        Some("url") => src.get("url")?.as_str()?.to_string(),
        _ => return None,
    };
    Some(json!({"type": "image_url", "image_url": {"url": url}}))
}

/// An Anthropic Messages request as a chat completions request for `model`.
pub fn request(body: &Value, model: &str, opts: &Options) -> Value {
    let mut messages: Vec<Value> = vec![];
    match body.get("system") {
        Some(Value::String(s)) if !s.is_empty() => messages.push(json!({"role": "system", "content": s})),
        Some(sys @ Value::Array(_)) => {
            let t = text_of_blocks(sys);
            if !t.is_empty() {
                messages.push(json!({"role": "system", "content": t}));
            }
        }
        _ => {}
    }
    for m in body.get("messages").and_then(Value::as_array).into_iter().flatten() {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
        let content = m.get("content").cloned().unwrap_or(Value::Null);
        let blocks = match &content {
            Value::String(s) => vec![json!({"type": "text", "text": s})],
            Value::Array(b) => b.clone(),
            _ => vec![],
        };
        if role == "assistant" {
            let text: Vec<&str> = blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect();
            let calls: Vec<Value> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
                .map(|b| {
                    json!({
                        "id": b.get("id").cloned().unwrap_or(json!("")),
                        "type": "function",
                        "function": {
                            "name": b.get("name").cloned().unwrap_or(json!("")),
                            "arguments": serde_json::to_string(b.get("input").unwrap_or(&json!({}))).unwrap_or_else(|_| "{}".into()),
                        }
                    })
                })
                .collect();
            let mut msg = json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text.join("")) }});
            if !calls.is_empty() {
                msg["tool_calls"] = json!(calls);
            }
            if text.is_empty() && calls.is_empty() {
                continue; // only thinking: nothing to send
            }
            messages.push(msg);
        } else {
            // Tool results go first, each its own `tool` message, right after the calls.
            for b in blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                let mut text = text_of_blocks(b.get("content").unwrap_or(&Value::Null));
                if b.get("is_error").and_then(Value::as_bool) == Some(true) {
                    text = format!("Error: {text}");
                }
                messages.push(json!({"role": "tool", "tool_call_id": b.get("tool_use_id").cloned().unwrap_or(json!("")), "content": text}));
            }
            let parts: Vec<Value> = blocks
                .iter()
                .filter_map(|b| match b.get("type").and_then(Value::as_str) {
                    Some("text") => Some(json!({"type": "text", "text": b.get("text").cloned().unwrap_or(json!(""))})),
                    Some("image") => image_part(b),
                    _ => None,
                })
                .collect();
            if parts.is_empty() {
                continue;
            }
            let only_text = parts.iter().all(|p| p["type"] == "text");
            let content = if only_text { json!(parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n")) } else { json!(parts) };
            messages.push(json!({"role": "user", "content": content}));
        }
    }

    let mut out = Map::new();
    out.insert("model".into(), json!(model));
    out.insert("messages".into(), json!(messages));
    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    out.insert("stream".into(), json!(stream));
    if stream {
        out.insert("stream_options".into(), json!({"include_usage": true}));
    }
    if let Some(n) = body.get("max_tokens") {
        out.insert(opts.max_tokens_field.clone(), n.clone());
    }
    for k in ["temperature", "top_p"] {
        if let Some(v) = body.get(k) {
            out.insert(k.into(), v.clone());
        }
    }
    if let Some(stop) = body.get("stop_sequences").filter(|s| s.as_array().is_some_and(|a| !a.is_empty())) {
        out.insert("stop".into(), stop.clone());
    }
    // Client tools only: server tools (web search and the like) have a `type`.
    let tools: Vec<Value> = body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|t| t.get("type").and_then(Value::as_str).is_none_or(|ty| ty == "custom"))
        .map(|t| {
            json!({"type": "function", "function": {
                "name": t.get("name").cloned().unwrap_or(json!("")),
                "description": t.get("description").cloned().unwrap_or(json!("")),
                "parameters": t.get("input_schema").cloned().unwrap_or(json!({"type": "object", "properties": {}})),
            }})
        })
        .collect();
    if !tools.is_empty() {
        out.insert("tools".into(), json!(tools));
        if let Some(choice) = body.get("tool_choice") {
            let c = match choice.get("type").and_then(Value::as_str) {
                Some("any") => json!("required"),
                Some("none") => json!("none"),
                Some("tool") => json!({"type": "function", "function": {"name": choice.get("name").cloned().unwrap_or(json!(""))}}),
                _ => json!("auto"),
            };
            out.insert("tool_choice".into(), c);
        }
    }
    // Structured output (the CLI asks for session titles this way).
    if let Some(schema) = body.pointer("/output_config/format").filter(|f| f.get("type").and_then(Value::as_str) == Some("json_schema")).and_then(|f| f.get("schema")) {
        out.insert("response_format".into(), json!({"type": "json_schema", "json_schema": {"name": "output", "schema": schema}}));
    }
    if opts.reasoning {
        if let Some(t) = body.get("thinking").filter(|t| t.get("type").and_then(Value::as_str) == Some("enabled")) {
            let budget = t.get("budget_tokens").and_then(Value::as_u64).unwrap_or(0);
            out.insert("reasoning_effort".into(), json!(if budget <= 4_096 { "low" } else if budget <= 16_384 { "medium" } else { "high" }));
        }
    }
    Value::Object(out)
}

pub fn stop_reason(finish: &str) -> &'static str {
    match finish {
        "length" => "max_tokens",
        "tool_calls" | "function_call" => "tool_use",
        _ => "end_turn",
    }
}

fn usage(u: Option<&Value>) -> Value {
    let prompt = u.and_then(|u| u.get("prompt_tokens")).and_then(Value::as_u64).unwrap_or(0);
    let cached = u.and_then(|u| u.pointer("/prompt_tokens_details/cached_tokens")).and_then(Value::as_u64).unwrap_or(0);
    let output = u.and_then(|u| u.get("completion_tokens")).and_then(Value::as_u64).unwrap_or(0);
    json!({"input_tokens": prompt.saturating_sub(cached), "cache_read_input_tokens": cached, "cache_creation_input_tokens": 0, "output_tokens": output})
}

/// A whole (not streamed) chat completion as an Anthropic message.
pub fn response(resp: &Value, model: &str) -> Value {
    let choice = resp.pointer("/choices/0").cloned().unwrap_or(Value::Null);
    let msg = choice.get("message").cloned().unwrap_or(Value::Null);
    let mut content = vec![];
    if let Some(r) = msg.get("reasoning_content").or(msg.get("reasoning")).and_then(Value::as_str).filter(|r| !r.is_empty()) {
        content.push(json!({"type": "thinking", "thinking": r, "signature": ""}));
    }
    if let Some(t) = msg.get("content").and_then(Value::as_str).filter(|t| !t.is_empty()) {
        content.push(json!({"type": "text", "text": t}));
    }
    for (i, c) in msg.get("tool_calls").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let args = c.pointer("/function/arguments").and_then(Value::as_str).unwrap_or("{}");
        content.push(json!({
            "type": "tool_use",
            "id": c.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| format!("toolu_fg_{i}")),
            "name": c.pointer("/function/name").cloned().unwrap_or(json!("")),
            "input": serde_json::from_str::<Value>(args).unwrap_or(json!({})),
        }));
    }
    json!({
        "id": format!("msg_{}", resp.get("id").and_then(Value::as_str).unwrap_or("forge")),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason(choice.get("finish_reason").and_then(Value::as_str).unwrap_or("stop")),
        "stop_sequence": null,
        "usage": usage(resp.get("usage")),
    })
}

#[derive(PartialEq, Clone, Copy)]
enum Open {
    None,
    Text,
    Thinking,
    /// The provider's tool-call index.
    Tool(u64),
}

/// Chat completion chunks in, Anthropic stream events out.
pub struct Stream {
    model: String,
    started: bool,
    index: u64,
    open: Open,
    /// The open tool call got no arguments yet.
    tool_args_seen: bool,
    tools_seen: Vec<u64>,
    finish: Option<String>,
    usage: Option<Value>,
    calls: u64,
}

impl Stream {
    pub fn new(model: &str) -> Stream {
        Stream { model: model.into(), started: false, index: 0, open: Open::None, tool_args_seen: false, tools_seen: vec![], finish: None, usage: None, calls: 0 }
    }

    fn start(&mut self, id: &str, out: &mut Vec<(String, Value)>) {
        if self.started {
            return;
        }
        self.started = true;
        out.push(("message_start".into(), json!({"type": "message_start", "message": {
            "id": format!("msg_{id}"), "type": "message", "role": "assistant", "model": self.model, "content": [],
            "stop_reason": null, "stop_sequence": null,
            "usage": {"input_tokens": 0, "output_tokens": 0},
        }})));
    }

    fn close(&mut self, out: &mut Vec<(String, Value)>) {
        if self.open == Open::None {
            return;
        }
        if matches!(self.open, Open::Tool(_)) && !self.tool_args_seen {
            out.push(("content_block_delta".into(), json!({"type": "content_block_delta", "index": self.index, "delta": {"type": "input_json_delta", "partial_json": "{}"}})));
        }
        if self.open == Open::Thinking {
            out.push(("content_block_delta".into(), json!({"type": "content_block_delta", "index": self.index, "delta": {"type": "signature_delta", "signature": ""}})));
        }
        out.push(("content_block_stop".into(), json!({"type": "content_block_stop", "index": self.index})));
        self.index += 1;
        self.open = Open::None;
    }

    fn open_block(&mut self, kind: Open, block: Value, out: &mut Vec<(String, Value)>) {
        self.close(out);
        out.push(("content_block_start".into(), json!({"type": "content_block_start", "index": self.index, "content_block": block})));
        self.open = kind;
    }

    fn delta(&self, delta: Value) -> (String, Value) {
        ("content_block_delta".into(), json!({"type": "content_block_delta", "index": self.index, "delta": delta}))
    }

    pub fn chunk(&mut self, c: &Value) -> Vec<(String, Value)> {
        let mut out = vec![];
        self.start(c.get("id").and_then(Value::as_str).unwrap_or("forge"), &mut out);
        if let Some(u) = c.get("usage").filter(|u| !u.is_null()) {
            self.usage = Some(u.clone());
        }
        let Some(choice) = c.pointer("/choices/0") else { return out };
        let d = choice.get("delta").cloned().unwrap_or(Value::Null);
        if let Some(r) = d.get("reasoning_content").or(d.get("reasoning")).and_then(Value::as_str).filter(|r| !r.is_empty()) {
            if self.open != Open::Thinking {
                self.open_block(Open::Thinking, json!({"type": "thinking", "thinking": "", "signature": ""}), &mut out);
            }
            out.push(self.delta(json!({"type": "thinking_delta", "thinking": r})));
        }
        if let Some(t) = d.get("content").and_then(Value::as_str).filter(|t| !t.is_empty()) {
            if self.open != Open::Text {
                self.open_block(Open::Text, json!({"type": "text", "text": ""}), &mut out);
            }
            out.push(self.delta(json!({"type": "text_delta", "text": t})));
        }
        for call in d.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
            let i = call.get("index").and_then(Value::as_u64).unwrap_or(0);
            if self.open != Open::Tool(i) {
                if self.tools_seen.contains(&i) {
                    continue; // a late fragment of a call already closed
                }
                self.tools_seen.push(i);
                self.calls += 1;
                let id = call.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| format!("toolu_fg_{}", self.calls));
                let name = call.pointer("/function/name").and_then(Value::as_str).unwrap_or("");
                self.open_block(Open::Tool(i), json!({"type": "tool_use", "id": id, "name": name, "input": {}}), &mut out);
                self.tool_args_seen = false;
            }
            if let Some(args) = call.pointer("/function/arguments").and_then(Value::as_str).filter(|a| !a.is_empty()) {
                self.tool_args_seen = true;
                out.push(self.delta(json!({"type": "input_json_delta", "partial_json": args})));
            }
        }
        if let Some(f) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish = Some(f.to_string());
        }
        out
    }

    /// The end of the upstream stream ([DONE], or it closed).
    pub fn finish(&mut self) -> Vec<(String, Value)> {
        let mut out = vec![];
        self.start("forge", &mut out);
        self.close(&mut out);
        let reason = stop_reason(self.finish.as_deref().unwrap_or("stop"));
        out.push(("message_delta".into(), json!({"type": "message_delta", "delta": {"stop_reason": reason, "stop_sequence": null}, "usage": usage(self.usage.as_ref())})));
        out.push(("message_stop".into(), json!({"type": "message_stop"})));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_conversation_becomes_chat_messages() {
        let body = json!({
            "model": "openai/gpt-x", "max_tokens": 1000, "stream": true,
            "system": [{"type": "text", "text": "Be brief."}],
            "thinking": {"type": "enabled", "budget_tokens": 10000},
            "tools": [
                {"name": "Write", "description": "Write a file", "input_schema": {"type": "object", "properties": {"file_path": {"type": "string"}}}},
                {"type": "web_search_20250305", "name": "web_search"},
            ],
            "tool_choice": {"type": "auto"},
            "messages": [
                {"role": "user", "content": "write hello"},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "hmm", "signature": "sig"},
                    {"type": "text", "text": "Writing."},
                    {"type": "tool_use", "id": "t1", "name": "Write", "input": {"file_path": "a"}},
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": [{"type": "text", "text": "ok"}]},
                    {"type": "text", "text": "thanks"},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAA"}},
                ]},
            ],
        });
        let r = request(&body, "gpt-x", &Options { max_tokens_field: "max_completion_tokens".into(), reasoning: true });
        assert_eq!(r["model"], "gpt-x");
        assert_eq!(r["max_completion_tokens"], 1000);
        assert_eq!(r["stream_options"], json!({"include_usage": true}));
        assert_eq!(r["reasoning_effort"], "medium");
        assert_eq!(r["tools"].as_array().unwrap().len(), 1, "server tools are left out");
        assert_eq!(r["tools"][0]["function"]["parameters"]["properties"]["file_path"]["type"], "string");
        assert_eq!(r["tool_choice"], "auto");
        let m = r["messages"].as_array().unwrap();
        assert_eq!(m[0], json!({"role": "system", "content": "Be brief."}));
        assert_eq!(m[1], json!({"role": "user", "content": "write hello"}));
        assert_eq!(m[2]["content"], "Writing.", "thinking is dropped");
        assert_eq!(m[2]["tool_calls"][0], json!({"id": "t1", "type": "function", "function": {"name": "Write", "arguments": "{\"file_path\":\"a\"}"}}));
        assert_eq!(m[3], json!({"role": "tool", "tool_call_id": "t1", "content": "ok"}));
        assert_eq!(m[4]["role"], "user");
        assert_eq!(m[4]["content"][1], json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,AAA"}}));
        let titled = request(&json!({"messages": [], "output_config": {"format": {"type": "json_schema", "schema": {"type": "object"}}}}), "m", &Options::default());
        assert_eq!(titled["response_format"], json!({"type": "json_schema", "json_schema": {"name": "output", "schema": {"type": "object"}}}));
        let plain = request(&body, "m", &Options::default());
        assert!(plain.get("reasoning_effort").is_none() && plain["max_tokens"] == 1000);
    }

    fn run(chunks: &[Value]) -> Vec<(String, Value)> {
        let mut s = Stream::new("openai/gpt-x");
        let mut out = vec![];
        for c in chunks {
            out.extend(s.chunk(c));
        }
        out.extend(s.finish());
        out
    }

    #[test]
    fn a_streamed_reply_with_thinking_text_and_two_tool_calls() {
        let ch = |delta: Value, finish: Value| json!({"id": "c1", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
        let out = run(&[
            ch(json!({"role": "assistant", "reasoning_content": "Let me "}), Value::Null),
            ch(json!({"reasoning_content": "think."}), Value::Null),
            ch(json!({"content": "Writing"}), Value::Null),
            ch(json!({"content": " it."}), Value::Null),
            ch(json!({"tool_calls": [{"index": 0, "id": "call_a", "type": "function", "function": {"name": "Write", "arguments": ""}}]}), Value::Null),
            ch(json!({"tool_calls": [{"index": 0, "function": {"arguments": "{\"file_path\":"}}]}), Value::Null),
            ch(json!({"tool_calls": [{"index": 0, "function": {"arguments": "\"a\"}"}}]}), Value::Null),
            ch(json!({"tool_calls": [{"index": 1, "function": {"name": "Read"}}]}), Value::Null),
            ch(json!({}), json!("tool_calls")),
            json!({"id": "c1", "choices": [], "usage": {"prompt_tokens": 100, "completion_tokens": 20, "prompt_tokens_details": {"cached_tokens": 40}}}),
        ]);
        let names: Vec<&str> = out.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, [
            "message_start",
            "content_block_start", "content_block_delta", "content_block_delta", "content_block_delta", "content_block_stop",
            "content_block_start", "content_block_delta", "content_block_delta", "content_block_stop",
            "content_block_start", "content_block_delta", "content_block_delta", "content_block_stop",
            "content_block_start", "content_block_delta", "content_block_stop",
            "message_delta", "message_stop",
        ]);
        assert_eq!(out[1].1["content_block"]["type"], "thinking");
        assert_eq!(out[2].1["delta"], json!({"type": "thinking_delta", "thinking": "Let me "}));
        assert_eq!(out[6].1["index"], 1);
        assert_eq!(out[10].1["content_block"], json!({"type": "tool_use", "id": "call_a", "name": "Write", "input": {}}));
        let args: String = out[11..13].iter().map(|(_, v)| v["delta"]["partial_json"].as_str().unwrap()).collect();
        assert_eq!(args, "{\"file_path\":\"a\"}");
        assert_eq!(out[14].1["content_block"]["id"], "toolu_fg_2", "a call without an id gets one");
        assert_eq!(out[15].1["delta"]["partial_json"], "{}", "a call without arguments gets an empty object");
        assert_eq!(out[17].1["delta"]["stop_reason"], "tool_use");
        assert_eq!(out[17].1["usage"], json!({"input_tokens": 60, "cache_read_input_tokens": 40, "cache_creation_input_tokens": 0, "output_tokens": 20}));
    }

    #[test]
    fn whole_replies_and_stop_reasons() {
        let r = response(&json!({"id": "x", "choices": [{"message": {"content": "Hi", "tool_calls": [{"id": "c", "function": {"name": "Read", "arguments": "{\"p\":1}"}}]}, "finish_reason": "tool_calls"}], "usage": {"prompt_tokens": 5, "completion_tokens": 2}}), "m");
        assert_eq!(r["content"][0], json!({"type": "text", "text": "Hi"}));
        assert_eq!(r["content"][1], json!({"type": "tool_use", "id": "c", "name": "Read", "input": {"p": 1}}));
        assert_eq!(r["stop_reason"], "tool_use");
        assert_eq!(stop_reason("length"), "max_tokens");
        assert_eq!(stop_reason("stop"), "end_turn");
        // An empty stream still makes a well-formed message.
        let names: Vec<String> = run(&[]).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["message_start", "message_delta", "message_stop"]);
    }
}
