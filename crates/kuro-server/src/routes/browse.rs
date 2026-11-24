//! Choosing a folder without typing its path.

use std::path::{Path, PathBuf};

use axum::extract::{Query, State};
use axum::Json;
use kuro_core::tools::files;
use kuro_core::KuroError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// Entries returned for one directory; beyond this, typing a path beats scrolling.
const MAX_ENTRIES: usize = 500;

/// How long the system folder dialog may go unanswered, so one that opened
/// behind the browser does not hold the request forever.
const DIALOG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Deserialize)]
pub struct BrowseQuery {
    /// Absent means the user's home.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub show_hidden: Option<bool>,
}

#[derive(Debug, Serialize)]
struct Entry {
    name: String,
    path: String,
    /// Whether anything is inside, so the UI can dim a dead end.
    has_children: bool,
}

/// A shortcut in the sidebar of the picker.
#[derive(Debug, Serialize)]
struct Shortcut {
    label: String,
    path: String,
}

/// Open the operating system's own folder chooser (macOS `osascript`), which people already know
/// how to use.
pub async fn native_picker(State(_state): State<SharedState>) -> AppResult<Json<Value>> {
    if !cfg!(target_os = "macos") {
        return Ok(Json(json!({
            "available": false,
            "reason": "The system folder chooser is only wired up on macOS.",
        })));
    }

    // `choose folder` returns an alias; POSIX path makes it usable as a workspace
    // root. Activating System Events first keeps the dialog from opening behind the browser.
    let script = r#"
        tell application "System Events" to activate
        set chosen to choose folder with prompt "Choose a project folder"
        return POSIX path of chosen
    "#;

    // A modal dialog waits forever and would hold the request open if ignored, so
    // the wait is bounded and timing out is reported as "nothing chosen", not a failure.
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
        // Cancelling is error -128, not a failure worth reporting.
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

    // `choose folder` yields a trailing slash; strip it for the stored root.
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

    // Canonicalise so `..` in a typed path resolves before listing and the path sent
    // back is the one a workspace will store.
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
        // Same refusal as the file tools: a picker should not list credential folders.
        if files::is_denied(&path) {
            continue;
        }

        entries.push(Entry {
            has_children: has_any_child(&path),
            name,
            path: path.to_string_lossy().to_string(),
        });
    }

    // Case-insensitive, so `Documents` and `bin` interleave naturally.
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

/// Places a project is likely to be. Only existing ones: dead buttons make the picker look broken.
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

    // Kuro's own data directory, for choosing where models go.
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

/// Expand a leading `~`.
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
