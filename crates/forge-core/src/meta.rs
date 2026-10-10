//! What the app keeps about sessions beyond the CLI's transcripts:
//! `<data>/sessions.json`, with the pinned and unread ones. (Archived ones
//! stay in `settings.json`; titles live in the transcripts.)

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::cli;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Marks {
    pub pinned: BTreeSet<String>,
    pub unread: BTreeSet<String>,
}

pub struct Meta {
    file: PathBuf,
    marks: Mutex<Marks>,
}

impl Meta {
    pub fn load(data: &Path) -> Meta {
        let file = data.join("sessions.json");
        let mut marks: Marks = fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        marks.pinned.retain(|id| cli::valid_session_id(id));
        marks.unread.retain(|id| cli::valid_session_id(id));
        Meta { file, marks: Mutex::new(marks) }
    }

    pub fn marks(&self) -> Marks {
        self.marks.lock().unwrap().clone()
    }

    fn change(&self, f: impl FnOnce(&mut Marks) -> bool) -> Result<(), String> {
        let mut m = self.marks.lock().unwrap();
        if !f(&mut m) {
            return Ok(());
        }
        if m.pinned.len() > 5000 || m.unread.len() > 5000 {
            return Err("too many marked sessions".into());
        }
        let text = serde_json::to_string_pretty(&*m).map_err(|e| e.to_string())?;
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, text).map_err(|e| e.to_string())?;
        fs::rename(tmp, &self.file).map_err(|e| e.to_string())
    }

    pub fn set_pinned(&self, id: &str, on: bool) -> Result<(), String> {
        if !cli::valid_session_id(id) {
            return Err("bad session id".into());
        }
        self.change(|m| if on { m.pinned.insert(id.into()) } else { m.pinned.remove(id) })
    }

    pub fn set_unread(&self, id: &str, on: bool) -> Result<(), String> {
        if !cli::valid_session_id(id) {
            return Err("bad session id".into());
        }
        self.change(|m| if on { m.unread.insert(id.into()) } else { m.unread.remove(id) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_round_trip_and_only_for_session_ids() {
        let dir = std::env::temp_dir().join(format!("forge-meta-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let id = "0e5b8a1c-1111-4222-8333-444455556666";
        let m = Meta::load(&dir);
        m.set_pinned(id, true).unwrap();
        m.set_unread(id, true).unwrap();
        assert!(m.set_pinned("../x", true).is_err());
        let back = Meta::load(&dir);
        assert!(back.marks().pinned.contains(id) && back.marks().unread.contains(id));
        back.set_unread(id, false).unwrap();
        assert!(Meta::load(&dir).marks().unread.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
