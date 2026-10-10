//! The app's settings: `<data>/settings.json`, plus endpoint keys in the OS
//! keychain (Windows Credential Manager, macOS Keychain), never on disk.
//!
//! An endpoint profile points the CLI at an Anthropic-compatible server
//! (Ollama, LM Studio, a gateway): the profile becomes `ANTHROPIC_*`
//! variables for the sessions it starts. With no profile the CLI uses its own
//! login (`claude login` or `ANTHROPIC_API_KEY`).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cli;

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
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    /// "apiKey" (x-api-key), "bearer" (Authorization), or "none".
    pub auth: String,
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
        let id_ok = !self.id.is_empty() && self.id.len() <= 64 && self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !id_ok {
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
        if !self.model.is_empty() && !cli::valid_model(&self.model) {
            return Err(format!("{}: that model name has characters a model name cannot have", self.name));
        }
        if !["apiKey", "bearer", "none"].contains(&self.auth.as_str()) {
            return Err(format!("{}: unknown authentication", self.name));
        }
        Ok(())
    }

    /// The variables a session started with this profile gets.
    pub fn env(&self, key: Option<String>) -> Vec<(String, String)> {
        let mut env = vec![("ANTHROPIC_BASE_URL".to_string(), self.base_url.trim_end_matches('/').to_string())];
        let key = key.unwrap_or_default();
        match self.auth.as_str() {
            "apiKey" => env.push(("ANTHROPIC_API_KEY".into(), key)),
            "bearer" => env.push(("ANTHROPIC_AUTH_TOKEN".into(), key)),
            // Local servers take any token; the CLI wants one to be set.
            _ => env.push(("ANTHROPIC_AUTH_TOKEN".into(), "none".into())),
        }
        if !self.model.is_empty() {
            for var in ["ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_OPUS_MODEL", "ANTHROPIC_DEFAULT_SONNET_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL", "ANTHROPIC_SMALL_FAST_MODEL"] {
                env.push((var.into(), self.model.clone()));
            }
        }
        env
    }
}

fn entry(profile_id: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &format!("endpoint:{profile_id}")).ok()
}

pub fn key(profile_id: &str) -> Option<String> {
    entry(profile_id)?.get_password().ok()
}

pub fn set_key(profile_id: &str, value: Option<&str>) -> Result<(), String> {
    let e = entry(profile_id).ok_or("the keychain is not available")?;
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if v.len() <= 4096 => e.set_password(v).map_err(|err| format!("could not save the key: {err}")),
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
        Profile { id: "local".into(), name: "Ollama".into(), base_url: "http://127.0.0.1:11434/".into(), model: "qwen3-coder:30b".into(), auth: "none".into(), has_key: false }
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
    fn a_profile_becomes_environment() {
        let env = profile().env(None);
        assert!(env.contains(&("ANTHROPIC_BASE_URL".into(), "http://127.0.0.1:11434".into())));
        assert!(env.contains(&("ANTHROPIC_AUTH_TOKEN".into(), "none".into())));
        assert!(env.contains(&("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), "qwen3-coder:30b".into())));
        let keyed = Profile { auth: "apiKey".into(), ..profile() }.env(Some("sk-1".into()));
        assert!(keyed.contains(&("ANTHROPIC_API_KEY".into(), "sk-1".into())));
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
