//! Reading and writing files: the containment layer under [`crate::workspace`],
//! its only caller. Chat has no file tools; everything that touches disk goes
//! through a workspace (a folder plus a mode).
//!
//! * **Tiers** ([`FileAccess`]): Ask is Off, Plan is Read, Agent is Write. Off
//!   means no tools are offered.
//! * **Roots**: for a workspace, exactly its own folder. Everything outside is
//!   refused, and a default `FilePermissions` grants nothing.
//! * **Paths are resolved, not compared.** `~/Projects/../.ssh/id_rsa` and a
//!   symlink to the keychain are "inside" `~/Projects` as strings. Every path is
//!   canonicalised (`..` collapsed, symlinks followed) before the root check, and
//!   roots are canonicalised too. For a file being created, the deepest existing
//!   ancestor is canonicalised instead.
//! * **Deny list inside the roots.** Granting `~` must not hand over SSH keys,
//!   cloud credentials, cookies or Kuro's own database. A safety net for a
//!   careless folder choice, not a security boundary.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::{KuroError, Result};

/// Largest file (bytes) that will be read into a reply.
pub const MAX_READ_BYTES: u64 = 256 * 1024;
/// Largest write (bytes). Over this is refused rather than silently truncated.
pub const MAX_WRITE_BYTES: usize = 1024 * 1024;
/// Entries listed for one directory before the rest are summarised.
const MAX_ENTRIES: usize = 200;

/// How much of the filesystem the model may touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileAccess {
    /// No file tools are offered. The default.
    #[default]
    Off,
    /// List and read within the granted folders.
    Read,
    /// List, read, create and overwrite within the granted folders.
    Write,
}

impl FileAccess {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "" => Some(Self::Off),
            "read" | "readonly" | "read_only" => Some(Self::Read),
            "write" | "readwrite" | "read_write" => Some(Self::Write),
            _ => None,
        }
    }

    pub fn allows_read(self) -> bool {
        matches!(self, Self::Read | Self::Write)
    }

    pub fn allows_write(self) -> bool {
        matches!(self, Self::Write)
    }

    /// What this tier permits, in the words shown next to the control.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Off => "No file access. The model is not offered file tools at all.",
            Self::Read => "The model can list folders and read files, inside the folders below only.",
            Self::Write => {
                "The model can list, read, create and overwrite files, inside the folders below \
                 only. It can change your files without asking first."
            }
        }
    }
}

/// The tier plus the folders it applies to. Built by
/// [`crate::workspace::Workspace::permissions`]; deliberately never loaded from
/// settings, so no stored grant can outlive what asked for it.
#[derive(Debug, Clone, Default)]
pub struct FilePermissions {
    pub access: FileAccess,
    /// Folders that may be reached. For a workspace, its own root and nothing else.
    pub roots: Vec<PathBuf>,
}

impl FilePermissions {
    /// Whether the file tools should be offered at all (a tier with no folders
    /// grants nothing).
    pub fn is_usable(&self) -> bool {
        self.access.allows_read() && !self.roots.is_empty()
    }

    /// The roots, canonicalised; roots that no longer exist are dropped.
    fn resolved_roots(&self) -> Vec<PathBuf> {
        self.roots
            .iter()
            .filter_map(|root| root.canonicalize().ok())
            .collect()
    }

    /// The granted folders as text.
    pub fn root_descriptions(&self) -> Vec<String> {
        self.roots
            .iter()
            .map(|root| root.display().to_string())
            .collect()
    }

    /// Resolve a path the model supplied into one it is allowed to use.
    ///
    /// `for_write` selects which tier is required and whether a path that does
    /// not exist yet is acceptable.
    pub fn resolve_path(&self, raw: &str, for_write: bool) -> Result<PathBuf> {
        if for_write {
            if !self.access.allows_write() {
                return Err(KuroError::bad_request(
                    "this workspace is not in Agent mode, so nothing can be changed. \
                     Switch it to Agent to allow edits.",
                ));
            }
        } else if !self.access.allows_read() {
            return Err(KuroError::bad_request(
                "this workspace is in Ask mode, so its files cannot be read. \
                 Switch it to Plan or Agent.",
            ));
        }

        let roots = self.resolved_roots();
        if roots.is_empty() {
            return Err(KuroError::bad_request(
                "this workspace has no folder to work in.",
            ));
        }

        let requested = expand_home(raw.trim());
        // Relative paths resolve against the first root.
        let absolute = if requested.is_absolute() {
            requested
        } else {
            roots[0].join(requested)
        };

        let resolved = canonicalise_for_access(&absolute, for_write)?;

        if !roots.iter().any(|root| resolved.starts_with(root)) {
            return Err(KuroError::bad_request(format!(
                "`{}` is outside every folder you granted. Granted: {}",
                absolute.display(),
                self.root_descriptions().join(", ")
            )));
        }

        if let Some(reason) = denied_reason(&resolved) {
            return Err(KuroError::bad_request(format!(
                "`{}` is refused: {reason}",
                resolved.display()
            )));
        }

        Ok(resolved)
    }
}

/// Expand a leading `~` against the home directory.
fn expand_home(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    let Some(rest) = trimmed.strip_prefix('~') else {
        return PathBuf::from(trimmed);
    };
    let Some(home) = home_dir() else {
        return PathBuf::from(trimmed);
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home
    } else {
        home.join(rest)
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Canonicalise a path, collapsing `..` and following symlinks. For a write the
/// file may not exist, so the deepest existing ancestor is canonicalised and the
/// rest appended, after rejecting `..` in it (`newdir/../../..` would rebuild the
/// escape).
fn canonicalise_for_access(path: &Path, for_write: bool) -> Result<PathBuf> {
    if let Ok(resolved) = path.canonicalize() {
        return Ok(resolved);
    }

    if !for_write {
        return Err(KuroError::not_found(format!("`{}`", path.display())));
    }
    // It exists yet does not canonicalise: a symlink to nowhere. A write would follow
    // it and create the target, wherever that is, so it is refused.
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(KuroError::bad_request(format!(
            "`{}` is a link to something that does not exist; it is not written through",
            path.display()
        )));
    }

    let mut trailing: Vec<&std::ffi::OsStr> = Vec::new();
    let mut current = path;

    loop {
        let Some(parent) = current.parent() else {
            return Err(KuroError::bad_request(format!(
                "`{}` has no folder that exists yet",
                path.display()
            )));
        };
        let Some(name) = current.file_name() else {
            return Err(KuroError::bad_request(format!(
                "`{}` is not a usable path",
                path.display()
            )));
        };
        trailing.push(name);

        if let Ok(resolved) = parent.canonicalize() {
            let mut out = resolved;
            for component in trailing.iter().rev() {
                out.push(component);
            }
            // `..` in the tail would undo the ancestor's canonicalisation.
            if out
                .components()
                .any(|component| matches!(component, Component::ParentDir))
            {
                return Err(KuroError::bad_request(format!(
                    "`{}` contains `..`, which is not allowed in a path to create",
                    path.display()
                )));
            }
            return Ok(out);
        }

        current = parent;
    }
}

/// Folder and file names that are refused wherever they appear.
const DENIED_COMPONENTS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
    ".docker",
    ".config/gh",
    "keychains",
    ".password-store",
    ".mozilla",
    ".bitcoin",
    ".ethereum",
    "cookies",
];

/// File names that are refused wherever they appear.
const DENIED_NAMES: &[&str] = &[
    "credentials.json",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    ".htpasswd",
    "shadow",
    "kuro.sqlite3",
];

/// Extensions that hold keys rather than content.
const DENIED_EXTENSIONS: &[&str] = &["pem", "key", "p12", "pfx", "keychain", "jks", "kdbx"];

/// Whether a path is on the deny list, whatever folder it sits in. Public so
/// other surfaces (the folder picker) apply the same list.
pub fn is_denied(path: &Path) -> bool {
    denied_reason(path).is_some()
}

/// Why a path inside a granted folder is still refused, if it is.
fn denied_reason(path: &Path) -> Option<&'static str> {
    let lowered = path.to_string_lossy().to_ascii_lowercase();
    // Normalise separators so a `.config/gh` style entry matches on any platform.
    let lowered = lowered.replace('\\', "/");

    for denied in DENIED_COMPONENTS {
        // Bounded by separators: `.ssh` must not match `my.ssh.notes`.
        if lowered.contains(&format!("/{denied}/")) || lowered.ends_with(&format!("/{denied}")) {
            return Some("it holds credentials or keys");
        }
    }

    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if DENIED_NAMES.contains(&name.as_str()) {
        return Some("it holds credentials or keys");
    }

    // `.env`, `.env.local`, `.env.production` and so on.
    if name == ".env" || name.starts_with(".env.") {
        return Some("it holds environment secrets");
    }

    if let Some(extension) = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase())
    {
        if DENIED_EXTENSIONS.contains(&extension.as_str()) {
            return Some("it looks like a private key or a key store");
        }
    }

    None
}

/// One entry in a listing.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub bytes: u64,
}

/// List a folder's contents.
pub fn list_directory(path: &Path) -> Result<Vec<Entry>> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| KuroError::other(format!("could not read `{}`: {error}", path.display())))?;

    if !metadata.is_dir() {
        return Err(KuroError::bad_request(format!(
            "`{}` is a file, not a folder. Use read_file for it.",
            path.display()
        )));
    }

    let mut entries: Vec<Entry> = std::fs::read_dir(path)
        .map_err(|error| KuroError::other(format!("could not list `{}`: {error}", path.display())))?
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let metadata = entry.metadata().ok();
            Entry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                bytes: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
            }
        })
        .collect();

    // Folders first, then alphabetical; stable across calls.
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(entries)
}

pub fn format_listing(path: &Path, entries: &[Entry]) -> String {
    if entries.is_empty() {
        return format!("`{}` is empty.", path.display());
    }

    let shown = entries.len().min(MAX_ENTRIES);
    let mut out = format!("Contents of `{}`:\n", path.display());

    for entry in entries.iter().take(shown) {
        if entry.is_dir {
            out.push_str(&format!("  {}/\n", entry.name));
        } else {
            out.push_str(&format!("  {} ({})\n", entry.name, human_bytes(entry.bytes)));
        }
    }

    if entries.len() > shown {
        out.push_str(&format!("  … and {} more\n", entries.len() - shown));
    }
    out
}

/// The whole file as text, for an undo snapshot: up to [`MAX_WRITE_BYTES`], beyond
/// what a model may read. `None` when it is larger, not text, or unreadable.
pub fn read_snapshot(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_WRITE_BYTES as u64 {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

/// Read a file as text.
pub fn read_file(path: &Path) -> Result<String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| KuroError::other(format!("could not read `{}`: {error}", path.display())))?;

    if metadata.is_dir() {
        return Err(KuroError::bad_request(format!(
            "`{}` is a folder. Use list_files for it.",
            path.display()
        )));
    }

    if metadata.len() > MAX_READ_BYTES {
        return Err(KuroError::bad_request(format!(
            "`{}` is {} — too large to read. The limit is {}.",
            path.display(),
            human_bytes(metadata.len()),
            human_bytes(MAX_READ_BYTES)
        )));
    }

    let bytes = std::fs::read(path)
        .map_err(|error| KuroError::other(format!("could not read `{}`: {error}", path.display())))?;

    // Lossy UTF-8 of a binary file would be noise; refuse instead.
    String::from_utf8(bytes).map_err(|_| {
        KuroError::bad_request(format!(
            "`{}` is not text, so there is nothing to read.",
            path.display()
        ))
    })
}

/// Write a file, creating it and any missing parent folders.
pub fn write_file(path: &Path, content: &str) -> Result<WriteReport> {
    if content.len() > MAX_WRITE_BYTES {
        return Err(KuroError::bad_request(format!(
            "that is {} of text, over the {} write limit.",
            human_bytes(content.len() as u64),
            human_bytes(MAX_WRITE_BYTES as u64)
        )));
    }

    if path.is_dir() {
        return Err(KuroError::bad_request(format!(
            "`{}` is a folder, so it cannot be written as a file.",
            path.display()
        )));
    }

    let existed = path.exists();
    let replaced_bytes = existed.then(|| std::fs::metadata(path).map(|m| m.len()).unwrap_or(0));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            KuroError::other(format!("could not create `{}`: {error}", parent.display()))
        })?;
    }

    std::fs::write(path, content).map_err(|error| {
        KuroError::other(format!("could not write `{}`: {error}", path.display()))
    })?;

    Ok(WriteReport {
        created: !existed,
        bytes: content.len() as u64,
        replaced_bytes,
    })
}

/// What a write did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReport {
    pub created: bool,
    pub bytes: u64,
    /// Size of what was overwritten, when something was.
    pub replaced_bytes: Option<u64>,
}

impl WriteReport {
    pub fn describe(&self, path: &Path) -> String {
        if self.created {
            format!(
                "Created `{}` ({}).",
                path.display(),
                human_bytes(self.bytes)
            )
        } else {
            format!(
                "Overwrote `{}` ({} replaced {}).",
                path.display(),
                human_bytes(self.bytes),
                human_bytes(self.replaced_bytes.unwrap_or(0))
            )
        }
    }
}

fn human_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;

    if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A permission set rooted at a fresh temporary folder.
    fn granted(access: FileAccess) -> (FilePermissions, PathBuf) {
        let root = std::env::temp_dir().join(format!("kuro-files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create root");
        // The temp dir is a symlink on macOS; canonicalise to compare like with like.
        let root = root.canonicalize().expect("canonicalise");
        (
            FilePermissions {
                access,
                roots: vec![root.clone()],
            },
            root,
        )
    }

    #[test]
    fn a_default_permission_object_grants_nothing() {
        let permissions = FilePermissions::default();

        assert_eq!(permissions.access, FileAccess::Off);
        assert!(permissions.roots.is_empty());
        assert!(!permissions.is_usable());
        assert!(permissions.resolve_path("/etc/hosts", false).is_err());
    }

    #[test]
    fn a_tier_with_no_folder_grants_nothing() {
        let permissions = FilePermissions {
            access: FileAccess::Write,
            roots: Vec::new(),
        };

        assert!(!permissions.is_usable());
        let error = permissions.resolve_path("/etc/hosts", false).unwrap_err().to_string();
        assert!(error.contains("no folder"), "got: {error}");
    }

    #[test]
    fn tiers_round_trip_and_reject_nonsense() {
        for access in [FileAccess::Off, FileAccess::Read, FileAccess::Write] {
            assert_eq!(FileAccess::parse(access.as_str()), Some(access));
            assert!(!access.describe().is_empty());
        }
        assert_eq!(FileAccess::parse("READ"), Some(FileAccess::Read));
        assert_eq!(FileAccess::parse("sudo"), None);
    }

    #[test]
    fn reading_is_allowed_inside_a_granted_folder() {
        let (permissions, root) = granted(FileAccess::Read);
        std::fs::write(root.join("note.txt"), "hello").expect("write");

        let resolved = permissions
            .resolve_path(root.join("note.txt").to_str().expect("path"), false)
            .expect("resolve");

        assert_eq!(read_file(&resolved).expect("read"), "hello");
    }

    #[test]
    fn a_relative_path_is_resolved_against_the_first_granted_folder() {
        let (permissions, root) = granted(FileAccess::Read);
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        std::fs::write(root.join("src/main.rs"), "fn main() {}").expect("write");

        let resolved = permissions.resolve_path("src/main.rs", false).expect("resolve");

        assert_eq!(resolved, root.join("src/main.rs"));
    }

    #[test]
    fn a_path_outside_every_root_is_refused() {
        let (permissions, _root) = granted(FileAccess::Write);

        let error = permissions.resolve_path("/etc/hosts", false).unwrap_err().to_string();

        assert!(error.contains("outside"), "got: {error}");
    }

    #[test]
    fn traversal_out_of_a_root_is_refused() {
        // Inside the root by string comparison, outside it in reality.
        let (permissions, root) = granted(FileAccess::Read);
        let escape = root.join("../../../etc/hosts");

        let error = permissions
            .resolve_path(escape.to_str().expect("path"), false)
            .unwrap_err()
            .to_string();

        assert!(
            error.contains("outside") || error.contains("`"),
            "traversal must not resolve: {error}"
        );
    }

    #[test]
    fn a_symlink_pointing_out_of_a_root_is_refused() {
        let (permissions, root) = granted(FileAccess::Read);
        let link = root.join("escape");
        // `/etc` exists and is not under the root.
        if std::os::unix::fs::symlink("/etc", &link).is_err() {
            return; // Symlinks unavailable; nothing to assert.
        }

        let error = permissions
            .resolve_path(link.join("hosts").to_str().expect("path"), false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("outside"), "a symlink must not be a way out: {error}");
    }

    #[test]
    fn writing_is_refused_at_the_read_only_tier() {
        let (permissions, root) = granted(FileAccess::Read);

        let error = permissions
            .resolve_path(root.join("new.txt").to_str().expect("path"), true)
            .unwrap_err()
            .to_string();

        assert!(error.contains("not in Agent mode"), "got: {error}");
    }

    #[test]
    fn a_file_that_does_not_exist_yet_can_be_written_but_not_read() {
        let (permissions, root) = granted(FileAccess::Write);
        let target = root.join("nested/deeper/new.txt");
        let as_text = target.to_str().expect("path");

        assert!(permissions.resolve_path(as_text, true).is_ok(), "a new file must resolve");
        assert!(
            permissions.resolve_path(as_text, false).is_err(),
            "reading something that does not exist is a not-found"
        );
    }

    #[test]
    fn a_new_path_cannot_traverse_out_through_a_folder_that_does_not_exist() {
        let (permissions, root) = granted(FileAccess::Write);
        let sneaky = root.join("newdir/../../../tmp/owned.txt");

        assert!(
            permissions.resolve_path(sneaky.to_str().expect("path"), true).is_err(),
            "`..` in the unresolved tail would undo the canonicalisation"
        );
    }

    #[test]
    fn credentials_are_refused_even_inside_a_granted_folder() {
        // Granting a broad folder must not hand over keys.
        let (permissions, root) = granted(FileAccess::Write);
        std::fs::create_dir_all(root.join(".ssh")).expect("mkdir");
        std::fs::write(root.join(".ssh/id_rsa"), "PRIVATE KEY").expect("write");
        std::fs::write(root.join(".env"), "SECRET=1").expect("write");
        std::fs::write(root.join("app.pem"), "cert").expect("write");

        for path in [".ssh/id_rsa", ".env", "app.pem"] {
            let error = permissions
                .resolve_path(root.join(path).to_str().expect("path"), false)
                .unwrap_err()
                .to_string();
            assert!(error.contains("refused"), "`{path}` should be refused, got: {error}");
        }
    }

    #[test]
    fn a_name_that_merely_contains_a_denied_word_is_allowed() {
        let (permissions, root) = granted(FileAccess::Read);
        std::fs::write(root.join("my.ssh.notes"), "not a key").expect("write");

        assert!(
            permissions
                .resolve_path(root.join("my.ssh.notes").to_str().expect("path"), false)
                .is_ok(),
            "the deny list must be bounded, not a substring match"
        );
    }

    #[test]
    fn a_file_too_large_to_read_is_refused_with_its_size() {
        let (_permissions, root) = granted(FileAccess::Read);
        let big = root.join("big.bin");
        std::fs::write(&big, vec![b'a'; (MAX_READ_BYTES + 1) as usize]).expect("write");

        let error = read_file(&big).unwrap_err().to_string();

        assert!(error.contains("too large"), "got: {error}");
    }

    #[test]
    fn a_binary_file_is_refused_rather_than_read_as_gibberish() {
        let (_permissions, root) = granted(FileAccess::Read);
        let binary = root.join("image.bin");
        std::fs::write(&binary, [0xff, 0xfe, 0x00, 0x01]).expect("write");

        let error = read_file(&binary).unwrap_err().to_string();

        assert!(error.contains("not text"), "got: {error}");
    }

    #[test]
    fn a_write_reports_whether_it_created_or_replaced() {
        let (_permissions, root) = granted(FileAccess::Write);
        let target = root.join("out/report.md");

        let created = write_file(&target, "first").expect("create");
        assert!(created.created);
        assert_eq!(created.replaced_bytes, None);
        assert!(created.describe(&target).contains("Created"));

        let replaced = write_file(&target, "second pass").expect("overwrite");
        assert!(!replaced.created);
        assert_eq!(replaced.replaced_bytes, Some(5));
        assert!(replaced.describe(&target).contains("Overwrote"));

        assert_eq!(std::fs::read_to_string(&target).expect("read"), "second pass");
    }

    #[test]
    fn an_oversized_write_is_refused_rather_than_truncated() {
        let (_permissions, root) = granted(FileAccess::Write);
        let content = "x".repeat(MAX_WRITE_BYTES + 1);

        let error = write_file(&root.join("huge.txt"), &content).unwrap_err().to_string();

        assert!(error.contains("write limit"), "got: {error}");
    }

    #[test]
    fn listings_put_folders_first_and_are_alphabetical() {
        let (_permissions, root) = granted(FileAccess::Read);
        std::fs::create_dir_all(root.join("zeta")).expect("mkdir");
        std::fs::write(root.join("alpha.txt"), "a").expect("write");

        let entries = list_directory(&root).expect("list");

        assert_eq!(entries[0].name, "zeta");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].name, "alpha.txt");
        assert!(format_listing(&root, &entries).contains("zeta/"));
    }

    #[test]
    fn listing_a_file_says_to_read_it_instead() {
        let (_permissions, root) = granted(FileAccess::Read);
        let file = root.join("note.txt");
        std::fs::write(&file, "hello").expect("write");

        let error = list_directory(&file).unwrap_err().to_string();

        assert!(error.contains("read_file"), "got: {error}");
    }

    #[test]
    fn a_tilde_is_expanded_to_the_home_directory() {
        let home = home_dir().expect("HOME must be set in tests");
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~/Projects"), home.join("Projects"));
        assert_eq!(expand_home("/absolute"), PathBuf::from("/absolute"));
    }

    #[test]
    fn sizes_are_written_for_a_person() {
        assert_eq!(human_bytes(512), "512 bytes");
        assert_eq!(human_bytes(2048), "2.0 KB");
        assert_eq!(human_bytes(3 * 1024 * 1024), "3.0 MB");
    }
}
