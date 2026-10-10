//! The app's settings: `<data>/settings.json`, plus endpoint keys in the OS
//! keychain (Windows Credential Manager, macOS Keychain), never on disk.
//!
//! A profile is a connected provider (from the catalog, or a custom URL).
//! Sessions routed to it go through the Forge gateway, which attaches the
//! key: the CLI only ever sees the gateway's token. With no profile the CLI
//! uses its own login (`claude login` or `ANTHROPIC_API_KEY`).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cli;
use crate::gateway;

const KEYCHAIN_SERVICE: &str = "com.msaid.forge";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub profiles: Vec<Profile>,
    /// The profile new sessions use; None: the CLI's own login.
    pub active_profile: Option<String>,
    /// Permission mode for new sessions.
    pub default_mode: String,
    /// Model for new sessions when no profile names one; None: the CLI's default.
    pub default_model: Option<String>,
    /// "system", "light" or "dark".
    pub theme: String,
    /// Conversations hidden from the sidebar (the files stay).
    pub archived: Vec<String>,
    pub sidebar_width: Option<u32>,
    pub changes_width: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    /// Also the model prefix in the gateway (`<id>/<model>`).
    pub id: String,
    pub name: String,
    /// The catalog preset it came from, if any.
    pub preset: Option<String>,
    /// "anthropic" (default) or "openai-chat".
    pub wire: String,
    pub base_url: String,
    pub model: String,
    /// For titles and other background calls; empty: `model`.
    pub fast_model: String,
    /// "apiKey" (x-api-key), "bearer" (Authorization), or "none".
    pub auth: String,
    /// openai-chat: "max_tokens" or "max_completion_tokens".
    pub max_tokens_field: Option<String>,
    /// openai-chat: the provider takes `reasoning_effort`.
    pub reasoning: bool,
    /// Read from the keychain for the window; never stored in the file.
    #[serde(skip_deserializing)]
    pub has_key: bool,
}

impl Settings {
    pub fn load(data: &Path) -> Settings {
        let mut s: Settings = fs::read_to_string(data.join("settings.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        s.normalize();
        s
    }

    pub fn save(&self, data: &Path) -> Result<(), String> {
        fs::create_dir_all(data).map_err(|e| e.to_string())?;
        let mut copy = self.clone();
        for p in &mut copy.profiles {
            p.has_key = false;
        }
        let text = serde_json::to_string_pretty(&copy).map_err(|e| e.to_string())?;
        let tmp = data.join("settings.json.tmp");
        fs::write(&tmp, text).map_err(|e| e.to_string())?;
        fs::rename(tmp, data.join("settings.json")).map_err(|e| e.to_string())
    }

    fn normalize(&mut self) {
        if !cli::MODES.contains(&self.default_mode.as_str()) {
            self.default_mode = "default".into();
        }
        if !["system", "light", "dark"].contains(&self.theme.as_str()) {
            self.theme = "system".into();
        }
        if self.default_model.as_deref().is_some_and(|m| !cli::valid_model(m)) {
            self.default_model = None;
        }
        for p in &mut self.profiles {
            // Profiles from before the gateway were Anthropic-compatible endpoints.
            if p.wire.is_empty() {
                p.wire = "anthropic".into();
            }
        }
        self.archived.retain(|id| cli::valid_session_id(id));
        self.archived.truncate(5000);
        self.sidebar_width = self.sidebar_width.map(|w| w.clamp(200, 480));
        self.changes_width = self.changes_width.map(|w| w.clamp(320, 900));
        if self.active_profile.as_ref().is_some_and(|id| !self.profiles.iter().any(|p| &p.id == id)) {
            self.active_profile = None;
        }
    }

    /// Settings from the window: checked, and kept only if valid.
    pub fn validated(mut self) -> Result<Settings, String> {
        if self.profiles.len() > 50 {
            return Err("too many profiles".into());
        }
        for p in &self.profiles {
            p.check()?;
        }
        let mut ids: Vec<&str> = self.profiles.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        if ids.len() != self.profiles.len() {
            return Err("two profiles share an id".into());
        }
        self.normalize();
        Ok(self)
    }

    pub fn active(&self) -> Option<&Profile> {
        let id = self.active_profile.as_ref()?;
        self.profiles.iter().find(|p| &p.id == id)
    }

    pub fn with_key_flags(mut self) -> Settings {
        for p in &mut self.profiles {
            p.has_key = p.auth != "none" && key(&p.id).is_some();
        }
        self
    }
}

impl Profile {
    fn check(&self) -> Result<(), String> {
        if !valid_profile_id(&self.id) {
            return Err("a profile id is letters, digits, - and _".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 80 {
            return Err("a profile needs a name of up to 80 characters".into());
        }
        let url_ok = (self.base_url.starts_with("http://") || self.base_url.starts_with("https://"))
            && self.base_url.len() <= 500
            && !self.base_url.chars().any(|c| c.is_whitespace() || c.is_control());
        if !url_ok {
            return Err(format!("{}: the base URL must start with http:// or https://", self.name));
        }
        for m in [&self.model, &self.fast_model] {
            if !m.is_empty() && !cli::valid_model(m) {
                return Err(format!("{}: that model name has characters a model name cannot have", self.name));
            }
        }
        if gateway::Wire::parse(&self.wire).is_none() {
            return Err(format!("{}: unknown wire", self.name));
        }
        if self.max_tokens_field.as_deref().is_some_and(|f| !["max_tokens", "max_completion_tokens"].contains(&f)) {
            return Err(format!("{}: unknown max tokens field", self.name));
        }
        if !["apiKey", "bearer", "none"].contains(&self.auth.as_str()) {
            return Err(format!("{}: unknown authentication", self.name));
        }
        Ok(())
    }

    /// Where the gateway sends this profile's requests.
    pub fn upstream(&self, key: Option<String>) -> gateway::Upstream {
        gateway::Upstream {
            wire: gateway::Wire::parse(&self.wire).unwrap_or(gateway::Wire::Anthropic),
            base_url: self.base_url.trim_end_matches('/').to_string(),
            auth: match self.auth.as_str() {
                "apiKey" => gateway::Auth::XApiKey,
                "bearer" => gateway::Auth::Bearer,
                _ => gateway::Auth::None,
            },
            key,
            chat: gateway::openai_chat::Options {
                max_tokens_field: self.max_tokens_field.clone().unwrap_or_else(|| "max_tokens".into()),
                reasoning: self.reasoning,
            },
        }
    }
}

/// A profile id: letters, digits, - and _ (it is a model prefix and a keychain name).
pub fn valid_profile_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn entry(profile_id: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &format!("endpoint:{profile_id}")).ok()
}

/// Debug builds only: keys for this run when the OS keychain refuses (a
/// Linux dev box without a keyring daemon). Release builds have no fallback.
#[cfg(debug_assertions)]
static DEV_KEYS: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);

pub fn key(profile_id: &str) -> Option<String> {
    #[cfg(debug_assertions)]
    if let Some(k) = DEV_KEYS.lock().unwrap().as_ref().and_then(|m| m.get(profile_id).cloned()) {
        return Some(k);
    }
    entry(profile_id)?.get_password().ok()
}

pub fn set_key(profile_id: &str, value: Option<&str>) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if let Some(m) = DEV_KEYS.lock().unwrap().as_mut() {
        m.remove(profile_id);
    }
    let e = entry(profile_id).ok_or("the keychain is not available")?;
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        // Read back: a keychain can accept a key and then not give it back.
        Some(v) if v.len() <= 4096 => match e.set_password(v).and_then(|_| match e.get_password() {
            Ok(back) if back == v => Ok(()),
            _ => Err(keyring::Error::NoEntry),
        }) {
            Ok(()) => Ok(()),
            #[cfg(debug_assertions)]
            Err(err) => {
                crate::log::write("warn", &format!("debug build: the keychain refused the key ({err}); keeping it in memory for this run"));
                DEV_KEYS.lock().unwrap().get_or_insert_with(Default::default).insert(profile_id.to_string(), v.to_string());
                Ok(())
            }
            #[cfg(not(debug_assertions))]
            Err(err) => Err(format!("the keychain did not keep the key: {err}")),
        },
        Some(_) => Err("that key is too long".into()),
        None => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        Profile { id: "local".into(), name: "Ollama".into(), wire: "anthropic".into(), base_url: "http://127.0.0.1:11434/".into(), model: "qwen3-coder:30b".into(), auth: "none".into(), ..Default::default() }
    }

    #[test]
    fn profiles_are_checked() {
        assert!(profile().check().is_ok());
        assert!(Profile { base_url: "file:///etc".into(), ..profile() }.check().is_err());
        assert!(Profile { id: "../x".into(), ..profile() }.check().is_err());
        assert!(Profile { model: "a b".into(), ..profile() }.check().is_err());
        assert!(Profile { auth: "magic".into(), ..profile() }.check().is_err());
        let twice = Settings { profiles: vec![profile(), profile()], ..Default::default() };
        assert!(twice.validated().is_err());
    }

    #[test]
    fn a_profile_becomes_an_upstream() {
        let up = profile().upstream(None);
        assert_eq!((up.wire, up.auth, up.base_url.as_str()), (gateway::Wire::Anthropic, gateway::Auth::None, "http://127.0.0.1:11434"));
        let chat = Profile { wire: "openai-chat".into(), auth: "bearer".into(), max_tokens_field: Some("max_completion_tokens".into()), reasoning: true, ..profile() }.upstream(Some("sk-1".into()));
        assert_eq!((chat.wire, chat.auth, chat.key.as_deref()), (gateway::Wire::OpenAiChat, gateway::Auth::Bearer, Some("sk-1")));
        assert_eq!(chat.chat.max_tokens_field, "max_completion_tokens");
        assert!(Profile { wire: "smoke-signals".into(), ..profile() }.check().is_err());
        // A profile saved before the gateway reads as an Anthropic-compatible one.
        let mut old: Settings = serde_json::from_str(r#"{"profiles":[{"id":"x","name":"Old","baseUrl":"http://h","model":"m","auth":"none"}]}"#).unwrap();
        old.normalize();
        assert_eq!(old.profiles[0].wire, "anthropic");
    }

    #[test]
    fn round_trip_without_keys_and_with_defaults() {
        let dir = std::env::temp_dir().join(format!("forge-settings-{}", std::process::id()));
        let mut s = Settings { profiles: vec![Profile { has_key: true, ..profile() }], active_profile: Some("local".into()), ..Default::default() };
        s.normalize();
        s.save(&dir).unwrap();
        let text = fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(!text.contains("hasKey\": true"));
        let back = Settings::load(&dir);
        assert_eq!(back.default_mode, "default");
        assert_eq!(back.theme, "system");
        assert_eq!(back.active().unwrap().name, "Ollama");
        let _ = fs::remove_dir_all(&dir);
    }
}
