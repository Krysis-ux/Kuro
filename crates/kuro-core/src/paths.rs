use std::path::{Path, PathBuf};

use crate::{KuroError, Result};

/// Where Kuro keeps its database, models, engine binaries and logs.
///
/// V1 targets macOS, so the default root is the standard Application Support
/// directory. `KURO_HOME` overrides it, which is what the test suite and any
/// future portable/packaged build use. When another platform is added, only
/// [`Paths::default_root`] needs a new branch.
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    /// Where weights go, when that is not inside the root.
    ///
    /// Model files are the only thing Kuro stores that is measured in tens of
    /// gigabytes, and a boot disk is often the wrong place for them. Everything
    /// else — the database, logs, the engine binary — is small enough that
    /// moving it would be fiddling rather than a decision, so only this one is
    /// overridable.
    models_override: Option<PathBuf>,
}

impl Paths {
    /// Resolve the app root without touching the filesystem.
    pub fn resolve() -> Result<Self> {
        let root = match std::env::var_os("KURO_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => Self::default_root()?,
        };
        Ok(Self { root, models_override: None })
    }

    /// Resolve the app root and create every directory Kuro writes to.
    pub fn resolve_and_create() -> Result<Self> {
        let paths = Self::resolve()?;
        paths.create_all()?;
        Ok(paths)
    }

    /// A Paths rooted at a specific directory, with no overrides.
    ///
    /// For tests and for any caller that already knows where the root is.
    /// Construction goes through here rather than through a public field so
    /// that an override can only be set by [`Self::with_models_dir`], which is
    /// also the thing that makes sure the directory exists.
    pub fn for_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            models_override: None,
        }
    }

    /// Point model downloads somewhere else.
    ///
    /// Applied once at startup rather than read per download, so a path that has
    /// become unwritable fails immediately and visibly instead of failing three
    /// gigabytes into a transfer. Existing weights are not moved, and are still
    /// found: the database stores each model's full path, not a path relative to
    /// this one.
    pub fn with_models_dir(mut self, dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        if dir.as_os_str().is_empty() {
            return Ok(self);
        }

        std::fs::create_dir_all(&dir).map_err(|error| {
            KuroError::bad_request(format!(
                "models directory `{}` could not be used: {error}",
                dir.display()
            ))
        })?;

        self.models_override = Some(dir);
        Ok(self)
    }

    fn default_root() -> Result<PathBuf> {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            KuroError::other("cannot determine home directory: HOME is not set")
        })?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Kuro"))
    }

    pub fn create_all(&self) -> Result<()> {
        for dir in [
            self.root.clone(),
            self.models_dir(),
            self.engine_dir(),
            self.engine_downloads_dir(),
            self.engine_versions_dir(),
            self.logs_dir(),
            self.attachments_dir(),
        ] {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }

    pub fn database_file(&self) -> PathBuf {
        self.root.join("kuro.sqlite3")
    }

    /// Downloaded GGUF weights.
    pub fn models_dir(&self) -> PathBuf {
        self.models_override
            .clone()
            .unwrap_or_else(|| self.root.join("models"))
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.root.join("engine")
    }

    /// Partially downloaded engine archives.
    pub fn engine_downloads_dir(&self) -> PathBuf {
        self.engine_dir().join("downloads")
    }

    /// Installed engine builds, one directory per release tag.
    pub fn engine_versions_dir(&self) -> PathBuf {
        self.engine_dir().join("versions")
    }

    pub fn engine_version_dir(&self, tag: &str) -> PathBuf {
        self.engine_versions_dir().join(sanitize_component(tag))
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Per-engine-process log file.
    pub fn engine_log_file(&self, model_id: &str) -> PathBuf {
        self.logs_dir()
            .join(format!("engine-{}.log", sanitize_component(model_id)))
    }

    pub fn attachments_dir(&self) -> PathBuf {
        self.root.join("attachments")
    }

    /// API keys and bearer tokens, kept out of the database so a copy of it never
    /// carries a secret. Written owner-only; see [`crate::secrets`].
    pub fn credentials_file(&self) -> PathBuf {
        self.root.join("credentials.json")
    }

    /// Local path for a model's weights, namespaced by model id.
    pub fn model_file(&self, model_id: &str, file_name: &str) -> PathBuf {
        self.models_dir()
            .join(sanitize_component(model_id))
            .join(sanitize_component(file_name))
    }
}

/// Make an arbitrary identifier safe to use as a single path component.
///
/// Model ids come from user input (a pasted Hugging Face repo path), so they
/// must never be able to escape the models directory or reach into a parent.
fn sanitize_component(raw: &str) -> String {
    let mut cleaned = String::with_capacity(raw.len());
    let mut previous_was_separator = false;

    for character in raw.chars() {
        match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '.' => {
                cleaned.push(character);
                previous_was_separator = false;
            }
            // Everything else collapses to a single dash so that a path like
            // `../../etc/passwd` reads as `etc-passwd` rather than keeping a
            // run of punctuation.
            _ => {
                if !previous_was_separator {
                    cleaned.push('-');
                    previous_was_separator = true;
                }
            }
        }
    }

    // A component of "." or ".." would still be traversal even after the
    // character filter above, and an empty component is not a valid name.
    let trimmed = cleaned.trim_matches(|c| c == '-' || c == '.');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// True when `candidate` stays inside `base` after normalisation.
///
/// Used when extracting engine archives, where a malicious entry could
/// otherwise write outside the destination directory.
pub fn is_contained(base: &Path, candidate: &Path) -> bool {
    let mut normalized = base.to_path_buf();
    for part in candidate.components() {
        match part {
            std::path::Component::Normal(p) => normalized.push(p),
            std::path::Component::CurDir => {}
            // Anything that walks upward or restarts from root is rejected
            // outright rather than resolved.
            _ => return false,
        }
    }
    normalized.starts_with(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_path_traversal_in_identifiers() {
        assert_eq!(sanitize_component("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_component(".."), "unnamed");
        assert_eq!(sanitize_component("."), "unnamed");
        assert_eq!(sanitize_component(""), "unnamed");
    }

    #[test]
    fn keeps_ordinary_model_identifiers_readable() {
        assert_eq!(
            sanitize_component("qwen3-4b-instruct.Q4_K_M.gguf"),
            "qwen3-4b-instruct.Q4_K_M.gguf"
        );
        // A repo path keeps its shape, with the separator flattened.
        assert_eq!(sanitize_component("unsloth/Qwen3-4B"), "unsloth-Qwen3-4B");
    }

    #[test]
    fn rejects_archive_entries_that_escape_the_destination() {
        let base = Path::new("/tmp/kuro/engine");
        assert!(is_contained(base, Path::new("bin/kuro-engine")));
        assert!(!is_contained(base, Path::new("../../../usr/bin/evil")));
        assert!(!is_contained(base, Path::new("/etc/passwd")));
    }

    #[test]
    fn weights_can_be_sent_to_another_disk() {
        let root = std::env::temp_dir().join(format!("kuro-paths-{}", uuid::Uuid::new_v4()));
        let elsewhere = root.join("external-drive");

        let paths = Paths::for_root(root.clone());
        assert_eq!(paths.models_dir(), root.join("models"));

        let moved = paths.with_models_dir(&elsewhere).expect("override");
        assert_eq!(moved.models_dir(), elsewhere);
        assert!(elsewhere.is_dir(), "the directory should be made ready to use");
        assert!(
            moved.model_file("qwen3-4b:q4_k_m", "x.gguf").starts_with(&elsewhere),
            "weights must land in the chosen directory, not the default one"
        );
        // Everything that is not weights stays with the database.
        assert!(moved.database_file().starts_with(&root));
        assert!(moved.logs_dir().starts_with(&root));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_empty_override_leaves_the_default_alone() {
        // A cleared setting is stored as an empty string, and reading that as
        // "put the models at the filesystem root" would be memorable.
        let paths = Paths::for_root("/tmp/kuro-test");
        let unchanged = paths.clone().with_models_dir("").expect("no-op");
        assert_eq!(unchanged.models_dir(), paths.models_dir());
    }

    #[test]
    fn model_file_stays_within_models_dir() {
        let paths = Paths::for_root(PathBuf::from("/tmp/kuro"));
        let file = paths.model_file("../../escape", "../../evil.gguf");
        assert!(file.starts_with(paths.models_dir()));
    }
}
