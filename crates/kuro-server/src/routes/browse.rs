//! Choosing a folder without typing its path.
//!
//! Kuro runs in a browser, and a browser cannot open a native folder picker that
//! returns a path — `<input webkitdirectory>` hands over the *files*, uploaded,
//! with their real location stripped. That is precisely backwards for a workspace,
//! where the path is the only thing wanted and copying the files would be absurd.
//!
//! So the picker is built rather than borrowed: this walks the daemon's own
//! filesystem and returns directory names, and the interface renders them as a
//! list you click through. It is the same trick a remote IDE uses, and it works
//! because the daemon is on the machine whose folders are being chosen.
//!
//! ## What it will not show
//!
//! Only directories, never file contents — this is for picking a place, not for
//! reading. Hidden directories are behind a flag, because `.config` is
//! occasionally the answer and `.git` never is. And the deny list from
//! [`files`] applies here too, so browsing cannot be used to confirm that
//! somebody's `.ssh` folder exists.

use std::path::{Path, PathBuf};

use axum::extract::{Query, State};
use axum::Json;
use kuro_core::tools::files;
use kuro_core::KuroError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// Entries returned for one directory.
///
/// A folder with more children than this is one nobody is scrolling through
/// anyway; the path field is there for that case.
const MAX_ENTRIES: usize = 500;

/// How long the system folder dialog may go unanswered.
///
/// Long enough to find the folder you want, short enough that a dialog which
/// opened behind the browser and was never noticed releases the request.
const DIALOG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Deserialize)]
pub struct BrowseQuery {
    /// Absent means "start somewhere sensible", which is the user's home.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub show_hidden: Option<bool>,
}

#[derive(Debug, Serialize)]
struct Entry {
    name: String,
    path: String,
    /// Whether anything is inside it, so the interface can dim a dead end
    /// rather than navigating into an empty list.
    has_children: bool,
}

/// A shortcut in the sidebar of the picker.
#[derive(Debug, Serialize)]
struct Shortcut {
    label: String,
    path: String,
}

/// Open the operating system's own folder chooser.
///
/// The built-in list below works everywhere and is nobody's first choice. On a
/// Mac the real dialog is one `osascript` call away, and it is the one people
/// actually know how to use: their sidebar, their recent places, their iCloud
/// folders, typing a path with `⌘⇧G`. The daemon is on the same machine as the
/// folders, so it can just ask for it.
///
/// The catch is that the dialog opens wherever the daemon's process is in the
/// window order, not in the browser — so this returns quickly with a clear
/// failure when nothing was chosen, and the interface keeps the list picker as
/// the way through if this is unavailable or gets dismissed.
pub async fn native_picker(State(_state): State<SharedState>) -> AppResult<Json<Value>> {
    if !cfg!(target_os = "macos") {
        return Ok(Json(json!({
            "available": false,
            "reason": "The system folder chooser is only wired up on macOS.",
        })));
    }

    // `choose folder` returns an alias; POSIX path turns it into something a
    // workspace root can be. Bringing the process to the front first is what
    // stops the dialog opening silently behind the browser.
    let script = r#"
        tell application "System Events" to activate
        set chosen to choose folder with prompt "Choose a project folder"
        return POSIX path of chosen
    "#;

    // A modal dialog waits forever, and a request that waits with it holds a
    // connection open for as long as the window is ignored. Observed the first
    // time this was called: the dialog opened behind the browser and the request
    // never returned. So the wait is bounded, and running out is reported as an
    // ordinary "nothing was chosen" rather than as a failure.
    let started = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();

    let output = match tokio::time::timeout(DIALOG_TIMEOUT, started).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(KuroError::other(format!("could not open the chooser: {error}")).into())
        }
        Err(_) => {
            return Ok(Json(json!({
                "available": true,
                "cancelled": true,
                "path": Value::Null,
                "reason": "The folder dialog was not answered. It may have opened behind \
                           this window — try again, or use Browse instead.",
            })))
        }
    };

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        // Cancelling is error -128 and is not a failure worth reporting as one.
        let cancelled = detail.contains("-128") || detail.trim().is_empty();
        return Ok(Json(json!({
            "available": true,
            "cancelled": cancelled,
            "path": Value::Null,
            "reason": if cancelled { Value::Null } else { json!(detail.trim()) },
        })));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Ok(Json(json!({ "available": true, "cancelled": true, "path": Value::Null })));
    }

    // `choose folder` yields a trailing slash, which is tidy in Finder and
    // untidy in a stored root.
    let path = path.trim_end_matches('/').to_string();

    Ok(Json(json!({ "available": true, "cancelled": false, "path": path })))
}

pub async fn browse(
    State(state): State<SharedState>,
    Query(query): Query<BrowseQuery>,
) -> AppResult<Json<Value>> {
    let requested = query
        .path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(expand_home)
        .unwrap_or_else(home);

    // Canonicalise so `..` in a typed path resolves before anything is listed,
    // and so the path sent back is the one the workspace will actually store.
    let directory = requested
        .canonicalize()
        .map_err(|error| KuroError::bad_request(format!("`{}` could not be opened: {error}", requested.display())))?;

    if !directory.is_dir() {
        return Err(KuroError::bad_request(format!(
            "`{}` is a file. Choose the folder that contains it.",
            directory.display()
        ))
        .into());
    }

    let show_hidden = query.show_hidden.unwrap_or(false);
    let mut entries: Vec<Entry> = Vec::new();

    let listing = std::fs::read_dir(&directory).map_err(|error| {
        KuroError::bad_request(format!("`{}` could not be read: {error}", directory.display()))
    })?;

    for item in listing.flatten() {
        let path = item.path();
        if !path.is_dir() {
            continue;
        }

        let name = item.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        // The same refusal the file tools apply. Browsing is not reading, but a
        // picker that lists somebody's credential folders is still telling a
        // model's user something they did not ask this screen for.
        if files::is_denied(&path) {
            continue;
        }

        entries.push(Entry {
            has_children: has_any_child(&path),
            name,
            path: path.to_string_lossy().to_string(),
        });
    }

    // Case-insensitive, so `Documents` and `bin` interleave the way a person
    // reading the list expects rather than splitting into two alphabets.
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    let total = entries.len();
    entries.truncate(MAX_ENTRIES);

    Ok(Json(json!({
        "path": directory.to_string_lossy(),
        "name": directory
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| directory.to_string_lossy().to_string()),
        "parent": directory.parent().map(|parent| parent.to_string_lossy().to_string()),
        "entries": entries,
        "truncated": total > MAX_ENTRIES,
        "shortcuts": shortcuts(&state),
    })))
}

/// The places a project is likely to be.
///
/// Only ones that exist. A shortcut to a folder that is not there is a dead
/// button, and a picker with three dead buttons reads as broken.
fn shortcuts(state: &SharedState) -> Vec<Shortcut> {
    let home = home();
    let mut found = vec![Shortcut {
        label: "Home".to_string(),
        path: home.to_string_lossy().to_string(),
    }];

    for (label, relative) in [
        ("Desktop", "Desktop"),
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("Projects", "Projects"),
        ("Code", "Code"),
        ("Developer", "Developer"),
        ("Repositories", "repos"),
    ] {
        let candidate = home.join(relative);
        if candidate.is_dir() {
            found.push(Shortcut {
                label: label.to_string(),
                path: candidate.to_string_lossy().to_string(),
            });
        }
    }

    // Where Kuro keeps its own data, because "where do models go" is a question
    // the storage setting exists to answer.
    if state.paths.root.is_dir() {
        found.push(Shortcut {
            label: "Kuro data".to_string(),
            path: state.paths.root.to_string_lossy().to_string(),
        });
    }

    found
}

/// Whether a directory has anything in it, without reading all of it.
fn has_any_child(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Expand a leading `~`, so a typed path means what it looks like.
fn expand_home(raw: &str) -> PathBuf {
    let Some(rest) = raw.strip_prefix('~') else {
        return PathBuf::from(raw);
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home()
    } else {
        home().join(rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tilde_path_means_the_home_directory() {
        assert_eq!(expand_home("~"), home());
        assert_eq!(expand_home("~/Projects"), home().join("Projects"));
        assert_eq!(expand_home("/tmp"), PathBuf::from("/tmp"));
    }

    #[test]
    fn an_empty_directory_is_marked_as_having_nothing_in_it() {
        let root = std::env::temp_dir().join(format!("kuro-browse-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("full/inside")).expect("mkdir");
        std::fs::create_dir_all(root.join("empty")).expect("mkdir");

        assert!(has_any_child(&root.join("full")));
        assert!(!has_any_child(&root.join("empty")));

        std::fs::remove_dir_all(&root).ok();
    }
}
