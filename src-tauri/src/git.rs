//! The Changes pane's data: the project's working tree against HEAD.
//!
//! Read-only. The page asks for the status, and for one file's before/after
//! text; a file it names must be one the status just listed, so the page
//! cannot read anything git does not already report as changed.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde::Serialize;

/// git's empty tree: the base of a repository with no commits yet.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// Bigger files are listed but not shown.
const MAX_SHOWN_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    /// The path before a rename.
    pub original_path: Option<String>,
    /// `added`, `modified`, `deleted`, `renamed`, `untracked`, `conflicted`.
    pub status: &'static str,
    pub additions: u64,
    pub deletions: u64,
    pub binary: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub files: Vec<ChangedFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub status: &'static str,
    pub original: String,
    pub modified: String,
    /// Binary or too large: listed, not shown.
    pub too_large_or_binary: bool,
}

fn git(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(cwd).args(args).env("GIT_OPTIONAL_LOCKS", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = command.output().map_err(|e| format!("could not run git: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn has_head(cwd: &Path) -> bool {
    git(cwd, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok()
}

fn base(cwd: &Path) -> &'static str {
    if has_head(cwd) { "HEAD" } else { EMPTY_TREE }
}

/// `git status --porcelain=v1 -z`: entries are `XY path\0`, and a rename or
/// copy is followed by its source path as its own `\0` field.
pub fn parse_porcelain(raw: &[u8]) -> Vec<(String, Option<String>, &'static str)> {
    let text = String::from_utf8_lossy(raw);
    let mut fields = text.split('\0').filter(|f| !f.is_empty());
    let mut out = Vec::new();
    while let Some(entry) = fields.next() {
        if entry.len() < 4 {
            continue;
        }
        let (x, y) = (entry.as_bytes()[0], entry.as_bytes()[1]);
        let path = entry[3..].to_string();
        let status = match (x, y) {
            (b'?', b'?') => "untracked",
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => "conflicted",
            (b'R', _) | (b'C', _) => "renamed",
            (b'A', _) => "added",
            (b'D', _) | (_, b'D') => "deleted",
            _ => "modified",
        };
        let original = if x == b'R' || x == b'C' { fields.next().map(String::from) } else { None };
        out.push((path, original, status));
    }
    out
}

/// `git diff --numstat -z`: `add\tdel\tpath\0`, or for a rename `add\tdel\t\0old\0new\0`.
/// Binary files show `-` for both counts.
pub fn parse_numstat(raw: &[u8]) -> HashMap<String, (u64, u64, bool)> {
    let text = String::from_utf8_lossy(raw);
    let mut fields = text.split('\0');
    let mut out = HashMap::new();
    while let Some(entry) = fields.next() {
        let mut parts = entry.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let path = if path.is_empty() {
            let _old = fields.next();
            match fields.next() {
                Some(new) => new.to_string(),
                None => continue,
            }
        } else {
            path.to_string()
        };
        let binary = a == "-" || d == "-";
        out.insert(path, (a.parse().unwrap_or(0), d.parse().unwrap_or(0), binary));
    }
    out
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

/// A path git reported, resolved inside the project. Refuses anything that
/// climbs out of it.
fn inside(root: &Path, relative: &str) -> Option<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute() || rel.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir)) {
        return None;
    }
    Some(root.join(rel))
}

pub fn status(cwd: &Path) -> Status {
    let Ok(top) = git(cwd, &["rev-parse", "--show-toplevel"]) else {
        return Status { is_repo: false, branch: None, files: vec![] };
    };
    let top = PathBuf::from(String::from_utf8_lossy(&top).trim());
    let branch = git(&top, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .map(|b| String::from_utf8_lossy(&b).trim().to_string())
        .filter(|b| !b.is_empty())
        .or_else(|| git(&top, &["rev-parse", "--short", "HEAD"]).ok().map(|b| String::from_utf8_lossy(&b).trim().to_string()));
    let entries = git(&top, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]).map(|r| parse_porcelain(&r)).unwrap_or_default();
    let counts = git(&top, &["diff", base(&top), "--numstat", "-z", "-M"]).map(|r| parse_numstat(&r)).unwrap_or_default();
    let files = entries
        .into_iter()
        .map(|(path, original_path, status)| {
            let (additions, deletions, binary) = match counts.get(&path) {
                Some(c) => *c,
                None if status == "untracked" => match inside(&top, &path).and_then(|p| std::fs::read(p).ok()) {
                    Some(bytes) if looks_binary(&bytes) => (0, 0, true),
                    Some(bytes) => (String::from_utf8_lossy(&bytes).lines().count() as u64, 0, false),
                    None => (0, 0, false),
                },
                None => (0, 0, false),
            };
            ChangedFile { path, original_path, status, additions, deletions, binary }
        })
        .collect();
    Status { is_repo: true, branch, files }
}

/// Before (HEAD) and after (working tree) text of one changed file.
pub fn file_diff(cwd: &Path, path: &str) -> Result<FileDiff, String> {
    let st = status(cwd);
    if !st.is_repo {
        return Err("not a git repository".into());
    }
    let file = st.files.into_iter().find(|f| f.path == path).ok_or("that file has no changes")?;
    let top = PathBuf::from(String::from_utf8_lossy(&git(cwd, &["rev-parse", "--show-toplevel"])?).trim());
    let before_path = file.original_path.clone().unwrap_or_else(|| file.path.clone());
    let original = if matches!(file.status, "untracked" | "added") || !has_head(&top) {
        Vec::new()
    } else {
        git(&top, &["show", &format!("HEAD:{before_path}")]).unwrap_or_default()
    };
    let modified = if file.status == "deleted" {
        Vec::new()
    } else {
        inside(&top, &file.path).and_then(|p| std::fs::read(p).ok()).unwrap_or_default()
    };
    let hidden = file.binary || looks_binary(&original) || looks_binary(&modified) || original.len() > MAX_SHOWN_BYTES || modified.len() > MAX_SHOWN_BYTES;
    Ok(FileDiff {
        path: file.path,
        status: file.status,
        original: if hidden { String::new() } else { String::from_utf8_lossy(&original).into_owned() },
        modified: if hidden { String::new() } else { String::from_utf8_lossy(&modified).into_owned() },
        too_large_or_binary: hidden,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_entries_and_renames() {
        let raw = b" M src/a.rs\0?? new.txt\0R  b2.rs\0b.rs\0D  gone.rs\0UU both.rs\0A  added.rs\0";
        let got = parse_porcelain(raw);
        assert_eq!(got[0], ("src/a.rs".into(), None, "modified"));
        assert_eq!(got[1], ("new.txt".into(), None, "untracked"));
        assert_eq!(got[2], ("b2.rs".into(), Some("b.rs".into()), "renamed"));
        assert_eq!(got[3], ("gone.rs".into(), None, "deleted"));
        assert_eq!(got[4], ("both.rs".into(), None, "conflicted"));
        assert_eq!(got[5], ("added.rs".into(), None, "added"));
    }

    #[test]
    fn numstat_counts_renames_and_binaries() {
        let raw = b"3\t1\tsrc/a.rs\0-\t-\timg.png\x002\t0\t\0old.rs\0new.rs\0";
        let got = parse_numstat(raw);
        assert_eq!(got["src/a.rs"], (3, 1, false));
        assert_eq!(got["img.png"], (0, 0, true));
        assert_eq!(got["new.rs"], (2, 0, false));
    }

    #[test]
    fn paths_stay_inside_the_project() {
        let root = Path::new("/work/p");
        assert!(inside(root, "src/a.rs").is_some());
        assert!(inside(root, "../secret").is_none());
        assert!(inside(root, "/etc/passwd").is_none());
    }

    #[test]
    fn a_real_repository() {
        let dir = std::env::temp_dir().join(format!("forge-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| git(&dir, args).unwrap();
        run(&["init", "-q"]);
        std::fs::write(dir.join("kept.txt"), "one\ntwo\n").unwrap();
        run(&["add", "."]);
        run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "seed"]);
        std::fs::write(dir.join("kept.txt"), "one\nTWO\nthree\n").unwrap();
        std::fs::write(dir.join("fresh.txt"), "a\nb\n").unwrap();

        let st = status(&dir);
        assert!(st.is_repo);
        let kept = st.files.iter().find(|f| f.path == "kept.txt").unwrap();
        assert_eq!((kept.status, kept.additions, kept.deletions), ("modified", 2, 1));
        let fresh = st.files.iter().find(|f| f.path == "fresh.txt").unwrap();
        assert_eq!((fresh.status, fresh.additions), ("untracked", 2));

        let d = file_diff(&dir, "kept.txt").unwrap();
        assert_eq!(d.original, "one\ntwo\n");
        assert_eq!(d.modified, "one\nTWO\nthree\n");
        assert!(file_diff(&dir, "../../etc/passwd").is_err(), "only files git lists");
        assert!(file_diff(&dir, ".git/config").is_err());
        assert!(!status(&std::env::temp_dir().join("forge-not-a-repo-xyz")).is_repo);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
