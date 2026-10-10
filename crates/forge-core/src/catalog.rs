//! Providers Forge knows how to reach (`catalog.json`, bundled, which a
//! `catalog.json` in the app's data folder extends or overrides by id), and
//! what a provider offers: its models, listed from its API. Listing is also
//! how a key is checked. Local servers are found by asking their default
//! ports, once, when the Providers page opens.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    /// "cloud", "local" or "custom".
    pub kind: String,
    /// "anthropic" or "openai-chat".
    pub wire: String,
    pub base_url: String,
    /// "apiKey" (x-api-key), "bearer" or "none".
    pub auth: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_url: Option<String>,
    /// Where the model list is, when not `<base>/models` (or `/v1/models`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_field: Option<String>,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub fn presets(data: &Path) -> Vec<Preset> {
    let mut all: Vec<Preset> = serde_json::from_str(include_str!("catalog.json")).expect("the bundled catalog parses");
    let user: Vec<Preset> = fs::read_to_string(data.join("catalog.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    for p in user.into_iter().filter(|p| crate::settings::valid_profile_id(&p.id)) {
        match all.iter_mut().find(|q| q.id == p.id) {
            Some(q) => *q = p,
            None => all.push(p),
        }
    }
    all
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,
    /// Dollars per million tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_price: Option<f64>,
    /// None: the provider doesn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<bool>,
}

fn per_million(v: Option<&Value>) -> Option<f64> {
    let per_token = match v? {
        Value::String(s) => s.parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    (per_token >= 0.0).then_some((per_token * 1_000_000.0 * 1000.0).round() / 1000.0)
}

/// Every shape seen in the wild: OpenAI `{data: [{id}]}` (OpenRouter adds
/// context, prices and supported parameters), Anthropic `{data: [{id,
/// display_name}]}`, Ollama `{models: [{name}]}`.
pub fn parse_models(v: &Value) -> Vec<ModelInfo> {
    let list = v.get("data").or(v.get("models")).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out: Vec<ModelInfo> = list
        .iter()
        .filter_map(|m| {
            let id = m.get("id").or(m.get("name")).or(m.get("model")).and_then(Value::as_str)?.to_string();
            let params: Vec<&str> = m.get("supported_parameters").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let has_params = m.get("supported_parameters").is_some();
            let modalities: Vec<&str> = m.pointer("/architecture/input_modalities").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            Some(ModelInfo {
                name: m.get("display_name").or(m.get("name")).and_then(Value::as_str).filter(|n| *n != id).map(String::from),
                context: m.get("context_length").or(m.get("context_window")).or(m.get("max_model_len")).and_then(Value::as_u64),
                input_price: per_million(m.pointer("/pricing/prompt")),
                output_price: per_million(m.pointer("/pricing/completion")),
                tools: has_params.then(|| params.contains(&"tools")),
                reasoning: has_params.then(|| params.contains(&"reasoning") || params.contains(&"include_reasoning")),
                vision: (!modalities.is_empty()).then(|| modalities.contains(&"image")),
                id,
            })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

/// Where the model list is, for a provider at `base_url` on `wire`.
pub fn models_url(wire: &str, base_url: &str, models_url: Option<&str>) -> String {
    if let Some(u) = models_url.filter(|u| !u.is_empty()) {
        return u.to_string();
    }
    let base = base_url.trim_end_matches('/');
    if wire == "anthropic" { format!("{base}/v1/models") } else { format!("{base}/models") }
}

/// List a provider's models. A refused key is an error that says so.
pub async fn list_models(client: &reqwest::Client, wire: &str, base_url: &str, models_url_override: Option<&str>, auth: &str, key: Option<&str>) -> Result<Vec<ModelInfo>, String> {
    let url = models_url(wire, base_url, models_url_override);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("the address must start with http:// or https://".into());
    }
    let mut rb = client.get(&url).timeout(Duration::from_secs(15));
    let key = key.unwrap_or_default();
    match auth {
        "apiKey" => rb = rb.header("x-api-key", key).header("anthropic-version", "2023-06-01"),
        "bearer" => rb = rb.bearer_auth(key),
        _ => {}
    }
    let resp = rb.send().await.map_err(|e| if e.is_connect() { format!("nothing answers at {url}") } else { format!("{url}: {e}") })?;
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(format!("the provider refused the key ({status})"));
    }
    if !resp.status().is_success() {
        return Err(format!("{url} answered {status}"));
    }
    let v: Value = resp.json().await.map_err(|e| format!("{url} did not answer with a model list: {e}"))?;
    let models = parse_models(&v);
    if models.is_empty() && v.get("data").is_none() && v.get("models").is_none() {
        return Err(format!("{url} did not answer with a model list"));
    }
    Ok(models)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub preset: String,
    pub name: String,
    pub base_url: String,
    pub models: Vec<String>,
}

/// Local servers answering on their default ports right now (each asked
/// once, with a short timeout, all at the same time).
pub async fn detect_local(client: &reqwest::Client, presets: &[Preset]) -> Vec<Detected> {
    let local: Vec<&Preset> = presets.iter().filter(|p| p.kind == "local").collect();
    let asks = local.iter().map(|p| async move {
        let url = models_url(&p.wire, &p.base_url, p.models_url.as_deref());
        let resp = client.get(&url).timeout(Duration::from_millis(600)).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let v: Value = resp.json().await.ok()?;
        let models = parse_models(&v).into_iter().map(|m| m.id).collect();
        Some(Detected { preset: p.id.clone(), name: p.name.clone(), base_url: p.base_url.clone(), models })
    });
    futures_util::future::join_all(asks).await.into_iter().flatten().collect()
}

/// Model lists per provider, kept a day (`<data>/models.json`).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ModelCache(HashMap<String, (u64, Vec<ModelInfo>)>);

const TTL: Duration = Duration::from_secs(24 * 3600);

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl ModelCache {
    pub fn load(data: &Path) -> ModelCache {
        fs::read(data.join("models.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self, data: &Path) -> Result<(), String> {
        fs::create_dir_all(data).map_err(|e| e.to_string())?;
        fs::write(data.join("models.json"), serde_json::to_vec(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
    /// The list if it is younger than a day.
    pub fn fresh(&self, provider: &str) -> Option<Vec<ModelInfo>> {
        self.0.get(provider).filter(|(at, _)| now().saturating_sub(*at) < TTL.as_secs()).map(|(_, m)| m.clone())
    }
    pub fn put(&mut self, provider: &str, models: Vec<ModelInfo>) {
        self.0.insert(provider.to_string(), (now(), models));
    }
    pub fn forget(&mut self, provider: &str) {
        self.0.remove(provider);
    }
}

/// A short JSON description of a provider for logs (no key).
pub fn describe(p: &Preset) -> Value {
    json!({"id": p.id, "wire": p.wire, "baseUrl": p.base_url})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_catalog_is_sound() {
        let all = presets(Path::new("/nonexistent"));
        assert!(all.len() >= 15);
        for p in &all {
            assert!(crate::settings::valid_profile_id(&p.id), "{}", p.id);
            assert!(["anthropic", "openai-chat"].contains(&p.wire.as_str()), "{}", p.id);
            assert!(["apiKey", "bearer", "none"].contains(&p.auth.as_str()), "{}", p.id);
            assert!(p.kind == "custom" || p.base_url.starts_with("https://") || (p.kind == "local" && p.base_url.starts_with("http://127.0.0.1:")), "{}", p.id);
        }
        let mut ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "ids are unique");
    }

    #[test]
    fn a_user_catalog_extends_and_overrides() {
        let dir = std::env::temp_dir().join(format!("forge-catalog-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("catalog.json"), r#"[{"id":"openai","name":"OpenAI (work proxy)","kind":"cloud","wire":"openai-chat","baseUrl":"https://proxy.corp/v1","auth":"bearer"},
            {"id":"mine","name":"Mine","kind":"custom","wire":"anthropic","baseUrl":"https://x","auth":"none"},{"id":"../bad","name":"x","kind":"cloud","wire":"anthropic","baseUrl":"https://x","auth":"none"}]"#).unwrap();
        let all = presets(&dir);
        assert_eq!(all.iter().find(|p| p.id == "openai").unwrap().base_url, "https://proxy.corp/v1");
        assert!(all.iter().any(|p| p.id == "mine"));
        assert!(!all.iter().any(|p| p.id == "../bad"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn model_lists_of_every_shape() {
        let openai = parse_models(&json!({"object": "list", "data": [{"id": "gpt-b"}, {"id": "gpt-a"}, {"id": "gpt-a"}]}));
        assert_eq!(openai.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["gpt-a", "gpt-b"]);
        let router = parse_models(&json!({"data": [{"id": "qwen/qwen3-coder", "name": "Qwen3 Coder", "context_length": 262144,
            "pricing": {"prompt": "0.0000002", "completion": "0.0000008"}, "supported_parameters": ["tools", "temperature"], "architecture": {"input_modalities": ["text"]}}]}));
        assert_eq!(router[0], ModelInfo { id: "qwen/qwen3-coder".into(), name: Some("Qwen3 Coder".into()), context: Some(262144), input_price: Some(0.2), output_price: Some(0.8), tools: Some(true), reasoning: Some(false), vision: Some(false) });
        let anthropic = parse_models(&json!({"data": [{"id": "claude-x", "display_name": "Claude X", "type": "model"}]}));
        assert_eq!(anthropic[0].name.as_deref(), Some("Claude X"));
        let ollama = parse_models(&json!({"models": [{"name": "qwen3-coder:30b", "model": "qwen3-coder:30b"}]}));
        assert_eq!(ollama[0].id, "qwen3-coder:30b");
        assert_eq!(models_url("anthropic", "https://api.anthropic.com/", None), "https://api.anthropic.com/v1/models");
        assert_eq!(models_url("openai-chat", "http://127.0.0.1:11434/v1", None), "http://127.0.0.1:11434/v1/models");
    }

    #[test]
    fn keys_are_checked_by_listing_models_and_local_servers_are_found() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/stub-openai.mjs");
        let Ok(mut child) = std::process::Command::new("node").arg(path).env("STUB_KEY", "sk-good").stdout(std::process::Stdio::piped()).spawn() else {
            return eprintln!("skipped: no node");
        };
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(child.stdout.take().unwrap()), &mut line).unwrap();
        let port: u16 = line.trim().strip_prefix("listening ").unwrap().parse().unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let client = reqwest::Client::new();
        let base = format!("http://127.0.0.1:{port}/v1");
        rt.block_on(async {
            let ok = list_models(&client, "openai-chat", &base, None, "bearer", Some("sk-good")).await.unwrap();
            assert_eq!(ok.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["gpt-stub", "gpt-stub-fast"]);
            let bad = list_models(&client, "openai-chat", &base, None, "bearer", Some("sk-bad")).await.unwrap_err();
            assert!(bad.contains("refused the key"), "{bad}");
            let gone = list_models(&client, "openai-chat", "http://127.0.0.1:9/v1", None, "none", None).await.unwrap_err();
            assert!(gone.contains("nothing answers"), "{gone}");
            // Detection: a "local" preset on the stub's port is found; a silent port is not.
            let presets = vec![
                Preset { id: "here".into(), name: "Here".into(), kind: "local".into(), wire: "openai-chat".into(), base_url: base.clone(), auth: "none".into(), key_url: None, models_url: None, max_tokens_field: None, reasoning: false, note: None },
                Preset { id: "gone".into(), name: "Gone".into(), kind: "local".into(), wire: "openai-chat".into(), base_url: "http://127.0.0.1:9/v1".into(), auth: "none".into(), key_url: None, models_url: None, max_tokens_field: None, reasoning: false, note: None },
            ];
            // The stub wants a key even for /models, as a protected server would: not found.
            assert!(detect_local(&client, &presets).await.is_empty());
        });
        let _ = child.kill();
        let _ = child.wait();
        // With no key required, it is found with its models.
        let Ok(mut open) = std::process::Command::new("node").arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/stub-openai.mjs")).env("STUB_KEY", "").env("STUB_OPEN", "1").stdout(std::process::Stdio::piped()).spawn() else { return };
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(open.stdout.take().unwrap()), &mut line).unwrap();
        let port: u16 = line.trim().strip_prefix("listening ").unwrap().parse().unwrap();
        let presets = vec![Preset { id: "here".into(), name: "Here".into(), kind: "local".into(), wire: "openai-chat".into(), base_url: format!("http://127.0.0.1:{port}/v1"), auth: "none".into(), key_url: None, models_url: None, max_tokens_field: None, reasoning: false, note: None }];
        let found = rt.block_on(detect_local(&client, &presets));
        assert_eq!(found, vec![Detected { preset: "here".into(), name: "Here".into(), base_url: format!("http://127.0.0.1:{port}/v1"), models: vec!["gpt-stub".into(), "gpt-stub-fast".into()] }]);
        let _ = open.kill();
        let _ = open.wait();
    }
}
