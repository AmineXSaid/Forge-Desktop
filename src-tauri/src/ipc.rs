//! The page side of the bridge.
//!
//! The page's `acquireVsCodeApi().postMessage(m)` (the initialization script in
//! `lib.rs`) calls `forge_send`. Host messages come back on a `Channel` the
//! page hands over with `forge_attach`, and are dispatched as `message` events,
//! exactly as VS Code delivers them to a webview.
//!
//! Every message is checked here before it reaches Node (plan §4.3). The host
//! still validates each request itself, as it does in VS Code; this check is
//! the envelope only.

use serde_json::Value;

/// The client message types (`WebViewToExtensionMessage`, Forge's
/// `src/shared/messages.ts`). Anything else is refused.
pub const CLIENT_TYPES: [&str; 7] = [
    "launch_claude",
    "io_message",
    "interrupt_claude",
    "close_channel",
    "request",
    "response",
    "cancel_request",
];

/// Ids are short strings the page makes up.
pub const MAX_ID_LEN: usize = 128;

/// One message, serialised. Attachments travel by path (`stage_attachment`),
/// so this only stops a runaway page.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

/// Check one page message and return it ready for the host: an object of a
/// known type, with ids of a sane size, and no `webviewId` (the host stamps
/// its own, as `WebViewService.registerWebview` does).
pub fn check_envelope(message: Value) -> Result<Value, String> {
    let mut message = match message {
        Value::Object(map) => map,
        _ => return Err("a page message must be an object".into()),
    };
    let kind = message
        .get("type")
        .and_then(Value::as_str)
        .ok_or("a page message needs a string type")?;
    if !CLIENT_TYPES.contains(&kind) {
        return Err(format!("unknown page message type: {}", truncate(kind, 64)));
    }
    for key in ["channelId", "requestId"] {
        match message.get(key) {
            None | Some(Value::Null) => {}
            Some(Value::String(s)) if s.len() <= MAX_ID_LEN => {}
            Some(Value::String(_)) => return Err(format!("{key} is longer than {MAX_ID_LEN} characters")),
            Some(_) => return Err(format!("{key} must be a string")),
        }
    }
    message.remove("webviewId");
    let message = Value::Object(message);
    let size = serde_json::to_string(&message).map(|s| s.len()).unwrap_or(usize::MAX);
    if size > MAX_MESSAGE_BYTES {
        return Err(format!("a page message of {size} bytes is over the {MAX_MESSAGE_BYTES}-byte limit"));
    }
    Ok(message)
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_each_client_type() {
        for kind in CLIENT_TYPES {
            assert!(check_envelope(json!({"type": kind, "channelId": "c1"})).is_ok(), "{kind}");
        }
    }

    #[test]
    fn refuses_what_is_not_a_client_message() {
        assert!(check_envelope(json!("request")).is_err());
        assert!(check_envelope(json!([1, 2])).is_err());
        assert!(check_envelope(json!({"channelId": "c1"})).is_err());
        assert!(check_envelope(json!({"type": 7})).is_err());
        assert!(check_envelope(json!({"type": "from-extension"})).is_err());
        assert!(check_envelope(json!({"type": "exec", "command": "rm -rf /"})).is_err());
    }

    #[test]
    fn refuses_bad_ids() {
        let long = "x".repeat(MAX_ID_LEN + 1);
        assert!(check_envelope(json!({"type": "request", "requestId": long})).is_err());
        assert!(check_envelope(json!({"type": "request", "requestId": 12})).is_err());
        assert!(check_envelope(json!({"type": "io_message", "channelId": {"a": 1}})).is_err());
        let max = "x".repeat(MAX_ID_LEN);
        assert!(check_envelope(json!({"type": "request", "requestId": max})).is_ok());
    }

    #[test]
    fn strips_a_page_supplied_webview_id() {
        let out = check_envelope(json!({"type": "request", "requestId": "r1", "webviewId": "sidebar:settings:"})).unwrap();
        assert!(out.get("webviewId").is_none());
        assert_eq!(out["requestId"], "r1");
    }

    #[test]
    fn refuses_an_oversized_message() {
        let big = "x".repeat(MAX_MESSAGE_BYTES);
        assert!(check_envelope(json!({"type": "request", "requestId": "r", "request": {"text": big}})).is_err());
    }
}
