//! Claude Code's own settings, the files the CLI reads and watches:
//!
//! | scope | file |
//! | --- | --- |
//! | user | `~/.claude/settings.json` |
//! | project | `<project>/.claude/settings.json` (shared with the team) |
//! | local | `<project>/.claude/settings.local.json` (just you) |
//!
//! The CLI applies local over project over user. Writes keep every key they
//! don't touch and are atomic. The window may write only the keys in
//! `rule_for`, each with its own value check: nothing here can turn on
//! bypass mode, and environment variables that would make tools load code
//! (`NODE_OPTIONS`, `LD_PRELOAD`, ...) are refused. Hooks, which make the CLI
//! run commands, are written through `add_hook`, which the app shell only
//! calls after the person confirms the exact command in a native dialog.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::cli;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scope {
    User,
    Project,
    Local,
}

impl Scope {
    pub fn parse(s: &str) -> Result<Scope, String> {
        match s {
            "user" => Ok(Scope::User),
            "project" => Ok(Scope::Project),
            "local" => Ok(Scope::Local),
            _ => Err("unknown settings scope".into()),
        }
    }
}

pub fn path(scope: Scope, project: Option<&Path>) -> Result<PathBuf, String> {
    match scope {
        Scope::User => cli::config_dir().map(|d| d.join("settings.json")).ok_or_else(|| "no home folder".into()),
        Scope::Project => project.map(|p| p.join(".claude").join("settings.json")).ok_or_else(|| "no project is open".into()),
        Scope::Local => project.map(|p| p.join(".claude").join("settings.local.json")).ok_or_else(|| "no project is open".into()),
    }
}

fn read_file(path: &Path) -> Result<Map<String, Value>, String> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(Map::new()),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(m)) => Ok(m),
            Ok(_) => Err(format!("{} is not a JSON object", path.display())),
            Err(e) => Err(format!("{} is not valid JSON: {e}", path.display())),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write_file(path: &Path, map: &Map<String, Value>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut text = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    text.push('\n');
    let tmp = path.with_extension("json.forge-tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllScopes {
    pub user: Value,
    pub project: Value,
    pub local: Value,
    pub has_project: bool,
    /// Whether Claude Code trusts the project folder. Until it does, it
    /// ignores the project's permission rules (`~/.claude.json`, read only).
    pub trusted: bool,
    /// A file that could not be read (bad JSON): shown, never overwritten.
    pub errors: Vec<String>,
}

pub fn read_all(project: Option<&Path>) -> AllScopes {
    let mut errors = vec![];
    let mut read = |scope| match path(scope, project).and_then(|p| read_file(&p)) {
        Ok(m) => Value::Object(m),
        Err(e) => {
            if project.is_some() || scope == Scope::User {
                errors.push(e);
            }
            json!({})
        }
    };
    let trusted = project.is_some_and(trusted);
    AllScopes { user: read(Scope::User), project: read(Scope::Project), local: read(Scope::Local), has_project: project.is_some(), trusted, errors }
}

/// `projects[<path>].hasTrustDialogAccepted` in `~/.claude.json`, for the
/// folder or one above it.
pub fn trusted(project: &Path) -> bool {
    let Some(state) = cli::home_dir().map(|h| h.join(".claude.json")).and_then(|p| fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
        return false;
    };
    project.ancestors().any(|dir| {
        state.get("projects").and_then(|p| p.get(dir.to_string_lossy().as_ref())).and_then(|p| p.get("hasTrustDialogAccepted")).and_then(Value::as_bool) == Some(true)
    })
}

enum Rule {
    Str(usize),
    Bool,
    OneOf(&'static [&'static str]),
    Int(i64, i64),
    List(usize),
    Env,
    /// One variable, `env.NAME`.
    EnvVar,
    /// A list that may only lose entries (approvals that make the CLI run something).
    Shrink,
    /// Only `false`: turning it on approves commands, which needs a confirmation.
    FalseOnly,
}

/// The keys the window may write, as dotted paths.
fn rule_for(key: &str) -> Option<Rule> {
    Some(match key {
        "language" | "outputStyle" | "plansDirectory" | "model" => Rule::Str(200),
        "attribution.commit" | "attribution.pr" => Rule::Str(2000),
        "respectGitignore" | "showTurnDuration" | "alwaysThinkingEnabled" | "disableAllHooks" | "includeCoAuthoredBy" => Rule::Bool,
        "sandbox.enabled" | "sandbox.autoAllowBashIfSandboxed" | "sandbox.allowUnsandboxedCommands" | "sandbox.network.allowLocalBinding" => Rule::Bool,
        "teammateMode" => Rule::OneOf(&["auto", "in-process", "tmux"]),
        "autoUpdatesChannel" => Rule::OneOf(&["stable", "latest"]),
        "forceLoginMethod" => Rule::OneOf(&["claudeai", "console"]),
        "effortLevel" => Rule::OneOf(&["low", "medium", "high", "xhigh", "max"]),
        // No bypassPermissions: turning checks off is done per session, in the window.
        "permissions.defaultMode" => Rule::OneOf(&["default", "acceptEdits", "plan", "auto", "dontAsk"]),
        "permissions.disableBypassPermissionsMode" => Rule::OneOf(&["disable"]),
        "cleanupPeriodDays" => Rule::Int(1, 36500),
        "sandbox.network.httpProxyPort" | "sandbox.network.socksProxyPort" => Rule::Int(1, 65535),
        "permissions.allow" | "permissions.ask" | "permissions.deny" | "permissions.additionalDirectories" | "sandbox.excludedCommands"
        | "sandbox.network.allowUnixSockets" | "companyAnnouncements" => Rule::List(1000),
        "env" => Rule::Env,
        k if k.starts_with("env.") => Rule::EnvVar,
        // Project MCP servers (.mcp.json): rejecting is free; approving runs the
        // server's command, so it goes through `approve_mcp` after a confirmation.
        "disabledMcpjsonServers" => Rule::List(1000),
        "enabledMcpjsonServers" => Rule::Shrink,
        "enableAllProjectMcpServers" => Rule::FalseOnly,
        _ => return None,
    })
}

/// Variables that would make a tool load or run code of their choosing.
const ENV_REFUSED: &[&str] = &[
    "NODE_OPTIONS", "LD_PRELOAD", "LD_LIBRARY_PATH", "BASH_ENV", "ENV", "PROMPT_COMMAND", "PYTHONSTARTUP", "PYTHONPATH",
    "PERL5OPT", "RUBYOPT", "JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "PATH", "PATHEXT", "COMSPEC", "SHELL", "GIT_SSH_COMMAND", "GIT_EXEC_PATH",
];

fn check_env_var(k: &str, v: &Value) -> Result<(), String> {
    let name_ok = !k.is_empty()
        && k.len() <= 100
        && k.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !name_ok {
        return Err(format!("`{k}` is not a variable name"));
    }
    let upper = k.to_ascii_uppercase();
    if ENV_REFUSED.contains(&upper.as_str()) || upper.starts_with("DYLD_") {
        return Err(format!("{k} can make tools run other code, so Forge does not set it; edit settings.json yourself if you need it"));
    }
    if !v.as_str().is_some_and(|s| s.len() <= 8192 && !s.chars().any(|c| c.is_control())) {
        return Err(format!("{k}: the value must be one line of text"));
    }
    Ok(())
}

/// `current`: the value the file holds now, for rules that compare with it.
fn check(rule: &Rule, key: &str, value: &Value, current: Option<&Value>) -> Result<(), String> {
    let ok = match (rule, value) {
        (Rule::Str(n), Value::String(s)) => s.len() <= *n && !s.chars().any(|c| c.is_control() && c != '\n'),
        (Rule::Bool, Value::Bool(_)) => true,
        (Rule::OneOf(set), Value::String(s)) => set.contains(&s.as_str()),
        (Rule::Int(lo, hi), Value::Number(n)) => n.as_i64().is_some_and(|v| v >= *lo && v <= *hi),
        (Rule::List(n), Value::Array(items)) => {
            items.len() <= *n && items.iter().all(|i| i.as_str().is_some_and(|s| !s.trim().is_empty() && s.len() <= 1000 && !s.chars().any(char::is_control)))
        }
        (Rule::Env, Value::Object(vars)) => {
            for (k, v) in vars {
                check_env_var(k, v)?;
            }
            vars.len() <= 500
        }
        (Rule::EnvVar, v) => return check_env_var(&key[4..], v),
        (Rule::Shrink, Value::Array(items)) => {
            let now: Vec<&Value> = current.and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
            items.iter().all(|i| now.contains(&i))
        }
        (Rule::FalseOnly, Value::Bool(false)) => true,
        _ => false,
    };
    if ok { Ok(()) } else { Err("that value does not fit this setting".into()) }
}

/// Set (or with `null`, remove) one dotted key in one scope's file.
pub fn set(scope: Scope, project: Option<&Path>, key: &str, value: Value) -> Result<(), String> {
    let rule = rule_for(key).ok_or_else(|| format!("Forge does not write `{key}`"))?;
    let file = path(scope, project)?;
    let mut root = read_file(&file)?;
    let parts: Vec<&str> = key.split('.').collect();
    if !value.is_null() {
        let mut current = root.get(parts[0]);
        for p in &parts[1..] {
            current = current.and_then(|v| v.get(*p));
        }
        check(&rule, key, &value, current)?;
    }
    set_path(&mut root, &parts, value);
    write_file(&file, &root)
}

fn set_path(map: &mut Map<String, Value>, parts: &[&str], value: Value) {
    let (first, rest) = (parts[0], &parts[1..]);
    if rest.is_empty() {
        if value.is_null() {
            map.remove(first);
        } else {
            map.insert(first.to_string(), value);
        }
        return;
    }
    let child = map.entry(first.to_string()).or_insert_with(|| json!({}));
    if !child.is_object() {
        *child = json!({});
    }
    let inner = child.as_object_mut().unwrap();
    set_path(inner, rest, value);
    if inner.is_empty() {
        map.remove(first);
    }
}

/// A project MCP server from `.mcp.json`, by name: its command line (or URL),
/// for the confirmation the app shell shows before `approve_mcp`.
pub fn project_mcp_target(project: &Path, name: &str) -> Result<String, String> {
    let mcp = fs::read_to_string(project.join(".mcp.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let mut found = vec![];
    mcp_from(mcp.as_ref().and_then(|m| m.get("mcpServers")), "project", &mut found);
    found.into_iter().find(|s| s.name == name).map(|s| s.target).ok_or_else(|| "no such server in .mcp.json".into())
}

/// Approve a project MCP server: the CLI will start it. Called only after the
/// person confirmed the command it runs.
pub fn approve_mcp(scope: Scope, project: Option<&Path>, name: &str) -> Result<(), String> {
    project_mcp_target(project.ok_or("no project is open")?, name)?;
    let file = path(scope, project)?;
    let mut root = read_file(&file)?;
    let list = root.entry("enabledMcpjsonServers").or_insert_with(|| json!([]));
    let Some(list) = list.as_array_mut() else { return Err("`enabledMcpjsonServers` is not a list".into()) };
    if !list.iter().any(|v| v == name) {
        list.push(json!(name));
    }
    if let Some(Value::Array(rejected)) = root.get_mut("disabledMcpjsonServers") {
        rejected.retain(|v| v != name);
    }
    write_file(&file, &root)
}

pub const HOOK_EVENTS: &[&str] = &["PreToolUse", "PostToolUse", "UserPromptSubmit", "Stop", "SubagentStop", "Notification", "SessionStart", "SessionEnd", "PreCompact"];

/// A hook, checked: the app shell asks the person before calling `add_hook`.
pub fn check_hook(event: &str, matcher: &str, command: &str, timeout: Option<u32>) -> Result<(), String> {
    if !HOOK_EVENTS.contains(&event) {
        return Err("unknown hook event".into());
    }
    if matcher.len() > 500 || matcher.chars().any(char::is_control) {
        return Err("bad matcher".into());
    }
    if command.trim().is_empty() || command.len() > 4000 || command.chars().any(|c| c.is_control() && c != '\t') {
        return Err("a hook needs a one-line command".into());
    }
    if timeout.is_some_and(|t| t == 0 || t > 3600) {
        return Err("the timeout is 1 to 3600 seconds".into());
    }
    Ok(())
}

pub fn add_hook(scope: Scope, project: Option<&Path>, event: &str, matcher: &str, command: &str, timeout: Option<u32>) -> Result<(), String> {
    check_hook(event, matcher, command, timeout)?;
    let file = path(scope, project)?;
    let mut root = read_file(&file)?;
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        return Err("`hooks` in that file is not an object; fix it by hand first".into());
    }
    let list = hooks.as_object_mut().unwrap().entry(event).or_insert_with(|| json!([]));
    let Some(list) = list.as_array_mut() else { return Err(format!("`hooks.{event}` is not a list")) };
    let mut h = json!({"type": "command", "command": command});
    if let Some(t) = timeout {
        h["timeout"] = json!(t);
    }
    list.push(json!({"matcher": matcher, "hooks": [h]}));
    write_file(&file, &root)
}

/// Remove one hook command, found by its event, group index and command index.
pub fn remove_hook(scope: Scope, project: Option<&Path>, event: &str, group: usize, index: usize) -> Result<(), String> {
    let file = path(scope, project)?;
    let mut root = read_file(&file)?;
    let groups = root.get_mut("hooks").and_then(|h| h.get_mut(event)).and_then(Value::as_array_mut).ok_or("no such hook")?;
    let g = groups.get_mut(group).ok_or("no such hook")?;
    let cmds = g.get_mut("hooks").and_then(Value::as_array_mut).ok_or("no such hook")?;
    if index >= cmds.len() {
        return Err("no such hook".into());
    }
    cmds.remove(index);
    if cmds.is_empty() {
        groups.remove(group);
    }
    write_file(&file, &root)
}

// ---------------------------------------------------------------- memory files

#[derive(Debug, Clone, Copy)]
pub enum Memory {
    User,
    Project,
    Local,
}

impl Memory {
    pub fn parse(s: &str) -> Result<Memory, String> {
        match s {
            "user" => Ok(Memory::User),
            "project" => Ok(Memory::Project),
            "local" => Ok(Memory::Local),
            _ => Err("unknown memory file".into()),
        }
    }
    pub fn path(self, project: Option<&Path>) -> Result<PathBuf, String> {
        match self {
            Memory::User => cli::config_dir().map(|d| d.join("CLAUDE.md")).ok_or_else(|| "no home folder".into()),
            Memory::Project => project.map(|p| p.join("CLAUDE.md")).ok_or_else(|| "no project is open".into()),
            Memory::Local => project.map(|p| p.join("CLAUDE.local.md")).ok_or_else(|| "no project is open".into()),
        }
    }
}

const MAX_MEMORY: usize = 1024 * 1024;

pub fn memory_read(which: Memory, project: Option<&Path>) -> Result<String, String> {
    match fs::read_to_string(which.path(project)?) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn memory_write(which: Memory, project: Option<&Path>, text: &str) -> Result<(), String> {
    if text.len() > MAX_MEMORY {
        return Err("a memory file is at most 1 MB".into());
    }
    let p = which.path(project)?;
    if text.trim().is_empty() && !p.exists() {
        return Ok(());
    }
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(p, text).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- extensions

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub name: String,
    pub description: String,
    /// "user" or "project".
    pub scope: &'static str,
    pub path: String,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
    pub skills: Vec<Extension>,
    pub agents: Vec<Extension>,
    pub commands: Vec<Extension>,
    pub mcp_servers: Vec<McpServer>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub name: String,
    /// "stdio", "http" or "sse".
    pub transport: String,
    /// The command line or the URL.
    pub target: String,
    /// "user", "project" (.mcp.json) or "local" (this project, just you).
    pub scope: &'static str,
}

/// The `description:` from a markdown file's front matter.
fn front_description(text: &str) -> String {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    for l in lines {
        if l.trim() == "---" {
            break;
        }
        if let Some(d) = l.strip_prefix("description:") {
            return d.trim().trim_matches('"').trim_matches('\'').chars().take(240).collect();
        }
    }
    String::new()
}

fn list_md(dir: &Path, scope: &'static str, out: &mut Vec<Extension>, nested_file: Option<&str>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let (name, file) = match nested_file {
            Some(f) if p.is_dir() => (e.file_name().to_string_lossy().to_string(), p.join(f)),
            None if p.extension().and_then(|x| x.to_str()) == Some("md") => (p.file_stem().unwrap_or_default().to_string_lossy().to_string(), p.clone()),
            _ => continue,
        };
        let Ok(text) = fs::read_to_string(&file) else { continue };
        out.push(Extension { name, description: front_description(&text), scope, path: file.display().to_string() });
    }
}

fn mcp_from(map: Option<&Value>, scope: &'static str, out: &mut Vec<McpServer>) {
    for (name, s) in map.and_then(Value::as_object).into_iter().flatten() {
        let transport = s.get("type").and_then(Value::as_str).unwrap_or(if s.get("url").is_some() { "http" } else { "stdio" }).to_string();
        let target = match s.get("url").and_then(Value::as_str) {
            Some(u) => u.to_string(),
            None => {
                let cmd = s.get("command").and_then(Value::as_str).unwrap_or_default();
                let args: Vec<&str> = s.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                std::iter::once(cmd).chain(args).collect::<Vec<_>>().join(" ")
            }
        };
        out.push(McpServer { name: name.clone(), transport, target, scope });
    }
}

pub fn extensions(project: Option<&Path>) -> Extensions {
    let mut x = Extensions::default();
    let roots: Vec<(PathBuf, &'static str)> = cli::config_dir().map(|d| (d, "user")).into_iter().chain(project.map(|p| (p.join(".claude"), "project"))).collect();
    for (root, scope) in &roots {
        list_md(&root.join("skills"), scope, &mut x.skills, Some("SKILL.md"));
        list_md(&root.join("agents"), scope, &mut x.agents, None);
        list_md(&root.join("commands"), scope, &mut x.commands, None);
    }
    // MCP: ~/.claude.json (user, and per project "local"), and the project's .mcp.json.
    let home_json = crate::cli::home_dir()
        .map(|h| h.join(".claude.json"))
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    if let Some(h) = &home_json {
        mcp_from(h.get("mcpServers"), "user", &mut x.mcp_servers);
        if let Some(p) = project {
            let key = p.display().to_string();
            mcp_from(h.get("projects").and_then(|ps| ps.get(&key)).and_then(|pj| pj.get("mcpServers")), "local", &mut x.mcp_servers);
        }
    }
    if let Some(p) = project {
        let mcp = fs::read_to_string(p.join(".mcp.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
        mcp_from(mcp.as_ref().and_then(|m| m.get("mcpServers")), "project", &mut x.mcp_servers);
    }
    for list in [&mut x.skills, &mut x.agents, &mut x.commands] {
        list.sort_by(|a, b| a.name.cmp(&b.name));
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("forge-cc-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn writes_keep_other_keys_and_nest_paths() {
        let p = tmp("nest");
        let file = path(Scope::Local, Some(&p)).unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, r#"{"theirs": {"keep": 1}, "permissions": {"allow": ["Read"]}}"#).unwrap();
        set(Scope::Local, Some(&p), "permissions.defaultMode", json!("plan")).unwrap();
        set(Scope::Local, Some(&p), "sandbox.network.httpProxyPort", json!(8080)).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["theirs"]["keep"], 1);
        assert_eq!(v["permissions"], json!({"allow": ["Read"], "defaultMode": "plan"}));
        assert_eq!(v["sandbox"]["network"]["httpProxyPort"], 8080);
        // null removes, and empty parents go with it
        set(Scope::Local, Some(&p), "sandbox.network.httpProxyPort", Value::Null).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert!(v.get("sandbox").is_none());
        let all = read_all(Some(&p));
        assert_eq!(all.local["permissions"]["defaultMode"], "plan");
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn only_listed_keys_and_good_values() {
        let p = tmp("rules");
        assert!(set(Scope::Project, Some(&p), "apiKeyHelper", json!("evil.sh")).is_err(), "runs a command");
        assert!(set(Scope::Project, Some(&p), "statusLine", json!({"command": "x"})).is_err());
        assert!(set(Scope::Project, Some(&p), "permissions.defaultMode", json!("bypassPermissions")).is_err());
        assert!(set(Scope::Project, Some(&p), "effortLevel", json!("huge")).is_err());
        assert!(set(Scope::Project, Some(&p), "cleanupPeriodDays", json!(0)).is_err());
        assert!(set(Scope::Project, Some(&p), "env", json!({"NODE_OPTIONS": "--require x"})).is_err());
        assert!(set(Scope::Project, Some(&p), "env", json!({"dyld_insert_libraries": "x"})).is_err());
        assert!(set(Scope::Project, Some(&p), "env", json!({"1BAD": "x"})).is_err());
        assert!(set(Scope::Project, Some(&p), "env", json!({"HTTPS_PROXY": "http://proxy:8080", "MAX_THINKING_TOKENS": "8000"})).is_ok());
        assert!(set(Scope::Project, Some(&p), "permissions.allow", json!(["Bash(npm test)", ""])).is_err());
        assert!(set(Scope::Project, None, "language", json!("french")).is_err(), "no project, no project scope");
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn single_variables_and_mcp_approvals() {
        let p = tmp("envvar");
        set(Scope::Local, Some(&p), "env.HTTPS_PROXY", json!("http://proxy:8080")).unwrap();
        set(Scope::Local, Some(&p), "env.NO_PROXY", json!("localhost")).unwrap();
        assert!(set(Scope::Local, Some(&p), "env.NODE_OPTIONS", json!("--require x")).is_err());
        assert!(set(Scope::Local, Some(&p), "env.Path", json!("x")).is_err(), "case-insensitive");
        assert!(set(Scope::Local, Some(&p), "env.A-B", json!("x")).is_err());
        assert!(set(Scope::Local, Some(&p), "env.X", json!(1)).is_err());
        set(Scope::Local, Some(&p), "env.NO_PROXY", Value::Null).unwrap();
        assert_eq!(read_all(Some(&p)).local["env"], json!({"HTTPS_PROXY": "http://proxy:8080"}));

        fs::write(p.join(".mcp.json"), r#"{"mcpServers": {"fs": {"command": "npx", "args": ["-y", "fs"]}, "db": {"command": "db-mcp"}}}"#).unwrap();
        assert!(set(Scope::Local, Some(&p), "enabledMcpjsonServers", json!(["fs"])).is_err(), "approving runs a command");
        assert!(set(Scope::Local, Some(&p), "enableAllProjectMcpServers", json!(true)).is_err());
        assert!(set(Scope::Local, Some(&p), "enableAllProjectMcpServers", json!(false)).is_ok());
        set(Scope::Local, Some(&p), "disabledMcpjsonServers", json!(["fs"])).unwrap();
        assert_eq!(project_mcp_target(&p, "fs").unwrap(), "npx -y fs");
        assert!(approve_mcp(Scope::Local, Some(&p), "nope").is_err());
        approve_mcp(Scope::Local, Some(&p), "fs").unwrap();
        approve_mcp(Scope::Local, Some(&p), "db").unwrap();
        let v = read_all(Some(&p)).local;
        assert_eq!(v["enabledMcpjsonServers"], json!(["fs", "db"]));
        assert_eq!(v["disabledMcpjsonServers"], json!([]), "approving takes it off the rejected list");
        // Removing an approval is fine; adding one back this way is not.
        set(Scope::Local, Some(&p), "enabledMcpjsonServers", json!(["db"])).unwrap();
        assert!(set(Scope::Local, Some(&p), "enabledMcpjsonServers", json!(["db", "fs"])).is_err());
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn hooks_are_added_and_removed() {
        let p = tmp("hooks");
        assert!(check_hook("Nope", "", "x", None).is_err());
        assert!(check_hook("PreToolUse", "Bash", "a\nb", None).is_err());
        add_hook(Scope::Local, Some(&p), "PreToolUse", "Bash", "echo hi", Some(10)).unwrap();
        let v = read_all(Some(&p)).local;
        assert_eq!(v["hooks"]["PreToolUse"][0], json!({"matcher": "Bash", "hooks": [{"type": "command", "command": "echo hi", "timeout": 10}]}));
        remove_hook(Scope::Local, Some(&p), "PreToolUse", 0, 0).unwrap();
        assert_eq!(read_all(Some(&p)).local["hooks"]["PreToolUse"], json!([]));
        assert!(remove_hook(Scope::Local, Some(&p), "PreToolUse", 0, 0).is_err());
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn bad_json_is_reported_not_overwritten() {
        let p = tmp("bad");
        let file = path(Scope::Project, Some(&p)).unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "{ not json").unwrap();
        assert!(set(Scope::Project, Some(&p), "language", json!("en")).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ not json");
        assert!(read_all(Some(&p)).errors.iter().any(|e| e.contains("not valid JSON")));
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn extensions_are_listed_with_descriptions() {
        let p = tmp("ext");
        fs::create_dir_all(p.join(".claude/skills/review")).unwrap();
        fs::write(p.join(".claude/skills/review/SKILL.md"), "---\nname: review\ndescription: Review a diff\n---\nbody").unwrap();
        fs::create_dir_all(p.join(".claude/commands")).unwrap();
        fs::write(p.join(".claude/commands/ship.md"), "Ship it").unwrap();
        fs::write(p.join(".mcp.json"), r#"{"mcpServers": {"docs": {"type": "http", "url": "https://x/mcp"}, "fs": {"command": "npx", "args": ["-y", "fs"]}}}"#).unwrap();
        let x = extensions(Some(&p));
        assert!(x.skills.contains(&Extension { name: "review".into(), description: "Review a diff".into(), scope: "project", path: p.join(".claude/skills/review/SKILL.md").display().to_string() }));
        assert!(x.commands.iter().any(|c| c.name == "ship" && c.description.is_empty()));
        assert!(x.mcp_servers.contains(&McpServer { name: "fs".into(), transport: "stdio".into(), target: "npx -y fs".into(), scope: "project" }));
        assert!(x.mcp_servers.iter().any(|m| m.name == "docs" && m.target == "https://x/mcp"));
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn trust_comes_from_the_cli_state_file() {
        let _env = crate::cli::tests_env::lock();
        let home = tmp("trust");
        let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let old = std::env::var_os(var);
        std::env::set_var(var, &home);
        let proj = home.join("work").join("app");
        assert!(!trusted(&proj), "no state file: not trusted");
        let key = home.join("work").display().to_string();
        fs::write(home.join(".claude.json"), json!({"projects": {key: {"hasTrustDialogAccepted": true}}}).to_string()).unwrap();
        assert!(trusted(&proj), "trusting a parent folder covers the project");
        assert!(!trusted(&home.join("elsewhere")));
        match old {
            Some(v) => std::env::set_var(var, v),
            None => std::env::remove_var(var),
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn memory_files_round_trip() {
        let p = tmp("mem");
        assert_eq!(memory_read(Memory::Project, Some(&p)).unwrap(), "");
        memory_write(Memory::Local, Some(&p), "# Notes\n").unwrap();
        assert_eq!(fs::read_to_string(p.join("CLAUDE.local.md")).unwrap(), "# Notes\n");
        memory_write(Memory::Project, Some(&p), "   ").unwrap();
        assert!(!p.join("CLAUDE.md").exists(), "an empty new file is not created");
        let _ = fs::remove_dir_all(&p);
    }
}
