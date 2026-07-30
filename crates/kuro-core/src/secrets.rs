//! Where API keys and bearer tokens live.
//!
//! Deliberately not the database. The SQLite file is the thing a user is most
//! likely to copy, back up, or hand over when reporting a bug, and a provider
//! key in it would travel along. Secrets go to a separate owner-only file, and
//! the database stores only a reference to the entry.
//!
//! The file is written with mode `0600` and replaced atomically, so a crash
//! mid-write cannot leave a half-truncated store behind. Values are never
//! returned to the browser — the API reports whether a reference is set, never
//! what it holds.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{KuroError, Result};

/// Owner read/write only. Anything broader would defeat the point of moving
/// secrets out of the database.
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;

#[derive(Clone)]
pub struct SecretStore {
    path: PathBuf,
}

impl SecretStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read one secret. `None` covers both "no store yet" and "no such entry",
    /// because callers treat them the same way.
    pub fn get(&self, reference: &str) -> Result<Option<String>> {
        Ok(self.read_all()?.remove(reference))
    }

    /// Store a secret under `reference`, replacing any existing value.
    ///
    /// An empty or whitespace-only value is rejected rather than stored, so a
    /// blank form field cannot silently turn a working connection into one that
    /// authenticates with nothing.
    pub fn put(&self, reference: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(KuroError::bad_request("the key is empty"));
        }

        let mut entries = self.read_all()?;
        entries.insert(reference.to_string(), value.trim().to_string());
        self.write_all(&entries)
    }

    pub fn delete(&self, reference: &str) -> Result<()> {
        let mut entries = self.read_all()?;
        if entries.remove(reference).is_none() {
            return Ok(());
        }
        self.write_all(&entries)
    }

    pub fn has(&self, reference: &str) -> bool {
        self.get(reference).ok().flatten().is_some()
    }

    fn read_all(&self) -> Result<BTreeMap<String, String>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_default()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(error.into()),
        }
    }

    /// Write via a temporary file in the same directory, then rename over the
    /// target. Rename within a directory is atomic, so readers see either the
    /// old store or the new one and never a partial write.
    fn write_all(&self, entries: &BTreeMap<String, String>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let temporary = self.path.with_extension("tmp");
        let encoded = serde_json::to_vec_pretty(entries)?;

        {
            let mut file = create_private(&temporary)?;
            file.write_all(&encoded)?;
            file.sync_all()?;
        }

        std::fs::rename(&temporary, &self.path)?;
        Ok(())
    }
}

#[cfg(unix)]
fn create_private(path: &Path) -> Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(FILE_MODE)
        .open(path)?)
}

#[cfg(not(unix))]
fn create_private(path: &Path) -> Result<std::fs::File> {
    // Kuro targets macOS today. On a platform without POSIX modes the file is
    // still outside the database, which is the main goal, but the permission
    // guarantee is weaker — worth tightening before shipping elsewhere.
    Ok(std::fs::File::create(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SecretStore {
        let path = std::env::temp_dir().join(format!("kuro-secrets-{}.json", uuid::Uuid::new_v4()));
        SecretStore::new(path)
    }

    #[test]
    fn round_trips_a_secret_and_reports_absence() {
        let store = store();
        assert_eq!(store.get("provider:openrouter").expect("get"), None);
        assert!(!store.has("provider:openrouter"));

        store.put("provider:openrouter", "sk-or-test").expect("put");

        assert_eq!(
            store.get("provider:openrouter").expect("get").as_deref(),
            Some("sk-or-test")
        );
        assert!(store.has("provider:openrouter"));
        std::fs::remove_file(&store.path).ok();
    }

    #[test]
    fn overwriting_one_entry_leaves_the_others_alone() {
        let store = store();
        store.put("a", "first").expect("put");
        store.put("b", "second").expect("put");
        store.put("a", "replaced").expect("overwrite");

        assert_eq!(store.get("a").expect("get").as_deref(), Some("replaced"));
        assert_eq!(store.get("b").expect("get").as_deref(), Some("second"));
        std::fs::remove_file(&store.path).ok();
    }

    #[test]
    fn deleting_is_idempotent() {
        let store = store();
        store.put("a", "value").expect("put");
        store.delete("a").expect("delete");
        store.delete("a").expect("second delete must not fail");
        assert_eq!(store.get("a").expect("get"), None);
        std::fs::remove_file(&store.path).ok();
    }

    #[test]
    fn refuses_to_store_a_blank_key() {
        let store = store();
        let error = store.put("a", "   ").unwrap_err().to_string();
        assert!(error.contains("empty"), "got: {error}");
        assert!(!store.has("a"), "a blank value must not create an entry");
    }

    #[test]
    fn trims_surrounding_whitespace_from_a_pasted_key() {
        let store = store();
        store.put("a", "  sk-pasted\n").expect("put");
        assert_eq!(store.get("a").expect("get").as_deref(), Some("sk-pasted"));
        std::fs::remove_file(&store.path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn the_store_is_not_world_readable() {
        use std::os::unix::fs::PermissionsExt;

        let store = store();
        store.put("a", "value").expect("put");

        let mode = std::fs::metadata(&store.path).expect("stat").permissions().mode();
        assert_eq!(mode & 0o077, 0, "group and other must have no access");
        std::fs::remove_file(&store.path).ok();
    }
}
