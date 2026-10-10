//! The Forge gateway: a loopback HTTP server that speaks the Anthropic
//! Messages API to the CLI and each provider's own API upstream.
//!
//! - It listens on 127.0.0.1 only, on a random port, and answers only
//!   requests that carry this launch's token. The CLI gets
//!   `ANTHROPIC_AUTH_TOKEN=<token>.<provider>`: the provider part names the
//!   route for models without a `provider/` prefix.
//! - Models are routed by prefix: `openrouter/qwen/qwen3-coder` goes to the
//!   `openrouter` provider as `qwen/qwen3-coder`.
//! - Keys come from `Resolve` (the keychain, in the app); they never reach
//!   the CLI.
//! - Wires: `anthropic` passes the request through (auth rewritten, the
//!   stream copied byte for byte, so prompt caching keeps working);
//!   `openai-chat` translates both ways (`openai_chat.rs`).
//! - Upstream errors come back as Anthropic errors with the same status, so
//!   the CLI's own retries (429, 529) keep working.

pub mod openai_chat;
pub mod sse;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::Response;
use axum::Router;
use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::log;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wire {
    Anthropic,
    OpenAiChat,
}

impl Wire {
    pub fn parse(s: &str) -> Option<Wire> {
        match s {
            "anthropic" => Some(Wire::Anthropic),
            "openai-chat" => Some(Wire::OpenAiChat),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Auth {
    /// `x-api-key: <key>`
    XApiKey,
    /// `Authorization: Bearer <key>`
    Bearer,
    None,
}

/// Where one provider's requests go.
#[derive(Debug, Clone)]
pub struct Upstream {
    pub wire: Wire,
    /// For `anthropic`, without `/v1` (`https://api.anthropic.com`); for
    /// `openai-chat`, up to the version (`https://api.openai.com/v1`).
    pub base_url: String,
    pub auth: Auth,
    pub key: Option<String>,
    pub chat: openai_chat::Options,
}

/// A provider id to its upstream, read when each request arrives (so a key
/// or URL changed in Settings applies to the next request).
pub type Resolve = Arc<dyn Fn(&str) -> Option<Upstream> + Send + Sync>;

struct Shared {
    token: String,
    resolve: Resolve,
    client: reqwest::Client,
}

pub struct Gateway {
    pub port: u16,
    token: String,
    _runtime: tokio::runtime::Runtime,
}

impl Gateway {
    pub fn start(resolve: Resolve) -> Result<Gateway, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("gateway")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let token = format!("fg{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .map_err(|e| e.to_string())?;
        let shared = Arc::new(Shared { token: token.clone(), resolve, client });
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))))
            .map_err(|e| format!("the gateway could not listen: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let app = Router::new().fallback(handle).with_state(shared).layer(DefaultBodyLimit::max(64 * 1024 * 1024));
        runtime.spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                log::write("error", &format!("gateway stopped: {e}"));
            }
        });
        log::write("info", &format!("gateway on 127.0.0.1:{port}"));
        Ok(Gateway { port, token, _runtime: runtime })
    }

    /// The variables a CLI routed through `provider` gets. `model` and
    /// `fast` are the provider's own names (they are prefixed here).
    pub fn env(&self, provider: &str, model: &str, fast: &str) -> Vec<(String, String)> {
        let mut env = vec![
            ("ANTHROPIC_BASE_URL".to_string(), format!("http://127.0.0.1:{}", self.port)),
            ("ANTHROPIC_AUTH_TOKEN".to_string(), format!("{}.{provider}", self.token)),
        ];
        let full = |m: &str| format!("{provider}/{m}");
        if !model.is_empty() {
            env.push(("ANTHROPIC_MODEL".into(), full(model)));
            env.push(("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), full(model)));
            env.push(("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), full(model)));
        }
        let fast = if fast.is_empty() { model } else { fast };
        if !fast.is_empty() {
            env.push(("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), full(fast)));
            env.push(("ANTHROPIC_SMALL_FAST_MODEL".into(), full(fast)));
        }
        env
    }

    /// Variables the CLI must not inherit when it goes through the gateway.
    pub const ENV_REMOVE: &'static [&'static str] = &["ANTHROPIC_API_KEY", "ANTHROPIC_MODEL", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX"];
}

/// An Anthropic-style error response.
pub fn error(status: StatusCode, kind: &str, message: &str) -> Response {
    let body = json!({"type": "error", "error": {"type": kind, "message": message}});
    Response::builder().status(status).header("content-type", "application/json").body(Body::from(body.to_string())).unwrap()
}

/// The Anthropic error type for an upstream status.
pub fn error_kind(status: u16) -> &'static str {
    match status {
        400 | 413 | 422 => "invalid_request_error",
        401 => "authentication_error",
        402 | 403 => "permission_error",
        404 => "not_found_error",
        429 => "rate_limit_error",
        503 | 529 => "overloaded_error",
        _ => "api_error",
    }
}

/// The launch token, and the default provider after it.
fn authorized<'a>(shared: &Shared, headers: &'a HeaderMap) -> Option<&'a str> {
    let given = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .or_else(|| headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")))?;
    let (token, provider) = given.split_once('.').unwrap_or((given, ""));
    // Same length and every byte compared, whatever matches first.
    let ok = token.len() == shared.token.len() && token.bytes().zip(shared.token.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
    ok.then_some(provider)
}

/// `provider/model` -> (provider, model); a model without a known prefix goes
/// to the default provider unchanged.
fn route<'a>(shared: &Shared, model: &'a str, default: &'a str) -> (String, &'a str) {
    if let Some((p, rest)) = model.split_once('/') {
        if !rest.is_empty() && (shared.resolve)(p).is_some() {
            return (p.to_string(), rest);
        }
    }
    (default.to_string(), model)
}

async fn handle(State(shared): State<Arc<Shared>>, method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Response {
    let started = std::time::Instant::now();
    let model = serde_json::from_slice::<Value>(&body).ok().and_then(|v| v.get("model").and_then(Value::as_str).map(String::from)).unwrap_or_default();
    let r = route_request(&shared, &method, &uri, &headers, &body).await;
    // One line per request: no content, no keys.
    log::write("gateway", &format!("{method} {} {model} -> {} in {} ms", uri.path(), r.status().as_u16(), started.elapsed().as_millis()));
    r
}

async fn route_request(shared: &Shared, method: &Method, uri: &Uri, headers: &HeaderMap, body: &Bytes) -> Response {
    let Some(default) = authorized(shared, headers) else {
        let has = |h: &str| if headers.contains_key(h) { "yes" } else { "no" };
        log::write("gateway", &format!("refused: wrong or missing token (x-api-key: {}, authorization: {})", has("x-api-key"), has("authorization")));
        return error(StatusCode::UNAUTHORIZED, "authentication_error", "Forge gateway: missing or wrong token");
    };
    let path = uri.path();
    let counting = path == "/v1/messages/count_tokens";
    if *method != Method::POST || !(path == "/v1/messages" || counting) {
        return error(StatusCode::NOT_FOUND, "not_found_error", &format!("Forge gateway does not serve {method} {path}"));
    }
    let Ok(mut req) = serde_json::from_slice::<Value>(body) else {
        return error(StatusCode::BAD_REQUEST, "invalid_request_error", "the request is not JSON");
    };
    let asked = req.get("model").and_then(Value::as_str).unwrap_or_default().to_string();
    let (provider, model) = route(shared, &asked, default);
    let Some(up) = (shared.resolve)(&provider) else {
        return error(StatusCode::BAD_REQUEST, "invalid_request_error", &format!("Forge gateway: no provider `{provider}` is connected"));
    };
    if up.auth != Auth::None && up.key.as_deref().is_none_or(str::is_empty) {
        return error(StatusCode::UNAUTHORIZED, "authentication_error", &format!("Forge has no key for `{provider}`: add it in Settings → Providers"));
    }
    let model = model.to_string();
    match up.wire {
        Wire::Anthropic => {
            req["model"] = json!(model);
            let url = format!("{}{}{}", up.base_url.trim_end_matches('/'), path, uri.query().map(|q| format!("?{q}")).unwrap_or_default());
            passthrough(shared, &up, &url, headers, req).await
        }
        Wire::OpenAiChat if counting => {
            // No counting endpoint: a local estimate (about 4 characters a token).
            let chars = ["system", "messages", "tools"].iter().map(|k| req.get(*k).map(|v| v.to_string().len()).unwrap_or(0)).sum::<usize>();
            Response::builder()
                .header("content-type", "application/json")
                .body(Body::from(json!({"input_tokens": chars / 4}).to_string()))
                .unwrap()
        }
        Wire::OpenAiChat => chat(shared, &up, &model, &asked, req).await,
    }
}

fn with_auth(mut rb: reqwest::RequestBuilder, up: &Upstream) -> reqwest::RequestBuilder {
    let key = up.key.clone().unwrap_or_default();
    match up.auth {
        Auth::XApiKey => rb = rb.header("x-api-key", key),
        Auth::Bearer => rb = rb.bearer_auth(key),
        Auth::None => {}
    }
    rb
}

/// Turn an upstream failure into the Anthropic error the CLI understands.
async fn upstream_error(resp: reqwest::Response) -> Response {
    let status = resp.status().as_u16();
    let retry = resp.headers().get("retry-after").cloned();
    let text = resp.text().await.unwrap_or_default();
    // Keep the provider's own message when it gives one.
    let message = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v.pointer("/error/message").or(v.get("message")).or(v.get("error")).and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| text.chars().take(500).collect());
    let mut r = error(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY), error_kind(status), &format!("provider: {message}"));
    if let Some(v) = retry {
        r.headers_mut().insert("retry-after", v);
    }
    r
}

fn unreachable(e: reqwest::Error) -> Response {
    // 529 makes the CLI back off and retry, as for an overloaded API.
    error(StatusCode::from_u16(529).unwrap(), "overloaded_error", &format!("Forge gateway could not reach the provider: {e}"))
}

async fn passthrough(shared: &Shared, up: &Upstream, url: &str, headers: &HeaderMap, req: Value) -> Response {
    let mut rb = shared.client.post(url).header("content-type", "application/json");
    for h in ["anthropic-version", "anthropic-beta"] {
        if let Some(v) = headers.get(h) {
            rb = rb.header(h, v);
        }
    }
    if !headers.contains_key("anthropic-version") {
        rb = rb.header("anthropic-version", "2023-06-01");
    }
    let resp = match with_auth(rb, up).body(req.to_string()).send().await {
        Ok(r) => r,
        Err(e) => return unreachable(e),
    };
    if !resp.status().is_success() {
        return upstream_error(resp).await;
    }
    let mut out = Response::builder().status(resp.status().as_u16());
    if let Some(ct) = resp.headers().get("content-type") {
        out = out.header("content-type", ct);
    }
    out.body(Body::from_stream(resp.bytes_stream())).unwrap()
}

async fn chat(shared: &Shared, up: &Upstream, model: &str, asked: &str, req: Value) -> Response {
    let stream = req.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let body = openai_chat::request(&req, model, &up.chat);
    let url = format!("{}/chat/completions", up.base_url.trim_end_matches('/'));
    let resp = match with_auth(shared.client.post(url).header("content-type", "application/json"), up).body(body.to_string()).send().await {
        Ok(r) => r,
        Err(e) => return unreachable(e),
    };
    if !resp.status().is_success() {
        return upstream_error(resp).await;
    }
    if !stream {
        let v: Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => return error(StatusCode::BAD_GATEWAY, "api_error", &format!("provider: unreadable reply: {e}")),
        };
        return Response::builder().header("content-type", "application/json").body(Body::from(openai_chat::response(&v, asked).to_string())).unwrap();
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);
    let asked = asked.to_string();
    tokio::spawn(async move {
        let mut parser = sse::Parser::default();
        let mut state = openai_chat::Stream::new(&asked);
        let mut upstream = resp.bytes_stream();
        let send = |events: Vec<(String, Value)>| {
            let text: String = events.iter().map(|(n, d)| sse::event(n, d)).collect();
            Bytes::from(text)
        };
        let mut done = false;
        while let Some(chunk) = upstream.next().await {
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    let err = sse::event("error", &json!({"type": "error", "error": {"type": "api_error", "message": format!("provider stream broke: {e}")}}));
                    let _ = tx.send(Ok(Bytes::from(err))).await;
                    return;
                }
            };
            for (_, data) in parser.feed(&bytes) {
                if data.trim() == "[DONE]" {
                    done = true;
                    break;
                }
                let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
                if let Some(e) = v.get("error") {
                    let msg = e.get("message").and_then(Value::as_str).unwrap_or("provider error").to_string();
                    let err = sse::event("error", &json!({"type": "error", "error": {"type": "api_error", "message": msg}}));
                    let _ = tx.send(Ok(Bytes::from(err))).await;
                    return;
                }
                let events = state.chunk(&v);
                if !events.is_empty() && tx.send(Ok(send(events))).await.is_err() {
                    return; // the CLI went away
                }
            }
            if done {
                break;
            }
        }
        let _ = tx.send(Ok(send(state.finish()))).await;
    });
    let body = Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx));
    let mut r = Response::builder().header("content-type", "text/event-stream").body(body).unwrap();
    r.headers_mut().insert("cache-control", HeaderValue::from_static("no-cache"));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared(token: &str) -> Shared {
        let resolve: Resolve = Arc::new(|p: &str| {
            (p == "openai" || p == "local").then(|| Upstream { wire: Wire::OpenAiChat, base_url: "http://x/v1".into(), auth: Auth::Bearer, key: None, chat: Default::default() })
        });
        Shared { token: token.into(), resolve, client: reqwest::Client::new() }
    }

    #[test]
    fn only_this_launch_token_gets_in() {
        let s = shared("fgsecret");
        let mut h = HeaderMap::new();
        assert_eq!(authorized(&s, &h), None);
        h.insert("authorization", "Bearer fgsecret.openai".parse().unwrap());
        assert_eq!(authorized(&s, &h), Some("openai"));
        h.insert("authorization", "Bearer fgsecreT.openai".parse().unwrap());
        assert_eq!(authorized(&s, &h), None);
        let mut k = HeaderMap::new();
        k.insert("x-api-key", "fgsecret".parse().unwrap());
        assert_eq!(authorized(&s, &k), Some(""));
    }

    #[test]
    fn models_route_by_prefix() {
        let s = shared("t");
        assert_eq!(route(&s, "openai/gpt-x", "local"), ("openai".to_string(), "gpt-x"));
        assert_eq!(route(&s, "qwen/qwen3-coder", "local"), ("local".to_string(), "qwen/qwen3-coder"), "an unknown prefix is part of the name");
        assert_eq!(route(&s, "claude-sonnet", "openai"), ("openai".to_string(), "claude-sonnet"));
        assert_eq!(error_kind(429), "rate_limit_error");
        assert_eq!(error_kind(401), "authentication_error");
        assert_eq!(error_kind(500), "api_error");
    }
}

#[cfg(test)]
mod e2e;
