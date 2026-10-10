//! Recent project folders: `<data>/recent.json`, newest first, at most 10.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MAX_RECENT: usize = 10;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Recent {
    #[serde(default)]
    pub folders: Vec<PathBuf>,
}

impl Recent {
    pub fn load(data_dir: &Path) -> Recent {
        fs::read_to_string(data_dir.join("recent.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// The newest folder that still exists.
    pub fn last(&self) -> Option<&PathBuf> {
        self.folders.iter().find(|p| p.is_dir())
    }

    pub fn push(&mut self, folder: &Path) {
        self.folders.retain(|p| p != folder);
        self.folders.insert(0, folder.to_path_buf());
        self.folders.truncate(MAX_RECENT);
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(data_dir)?;
        let file = data_dir.join("recent.json");
        let tmp = data_dir.join("recent.json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self).unwrap_or_default())?;
        fs::rename(tmp, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_without_duplicates_and_capped() {
        let mut r = Recent::default();
        for i in 0..12 {
            r.push(Path::new(&format!("/p{i}")));
        }
        r.push(Path::new("/p5"));
        assert_eq!(r.folders.len(), MAX_RECENT);
        assert_eq!(r.folders[0], PathBuf::from("/p5"));
        assert_eq!(r.folders.iter().filter(|p| p.as_path() == Path::new("/p5")).count(), 1);
    }

    #[test]
    fn round_trips_and_skips_missing_folders() {
        let dir = std::env::temp_dir().join(format!("forge-recent-{}", std::process::id()));
        let mut r = Recent::default();
        r.push(&dir);
        r.push(Path::new("/definitely/not/here"));
        r.save(&dir).unwrap();
        let back = Recent::load(&dir);
        assert_eq!(back, r);
        assert_eq!(back.last(), Some(&dir));
        let _ = fs::remove_dir_all(&dir);
    }
}
