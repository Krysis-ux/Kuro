//! Obtaining the engine binary.
//!
//! Kuro does not build its inference engine. On first use it downloads the
//! prebuilt release that matches this machine, extracts it into a per-tag
//! directory and records it, so later launches reuse the cached copy. The
//! extracted executable is renamed to `kuro-engine`, which is the name it runs
//! under for the rest of its life — in the process list, in logs, and in
//! anything the user is shown.
//!
//! The release tag is pinned to one that has been tested rather than tracking
//! `latest`, because upstream asset naming changes from time to time and a
//! silent rename would otherwise break model loading for everyone at once.
//! Settings offers an explicit "check for engine updates" action.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Deserialize;

use crate::catalog::download::download_to_file;
use crate::db::{Db, EngineRuntimeRecord};
use crate::paths::{is_contained, Paths};
use crate::{KuroError, Result};

/// Engine release Kuro ships against. Verified to run on macOS arm64.
pub const DEFAULT_ENGINE_TAG: &str = "b10182";

/// Name the engine executable runs under once Kuro has installed it.
pub const ENGINE_BINARY_NAME: &str = "kuro-engine";

/// The upstream name inside the release archive, before Kuro renames it.
const UPSTREAM_BINARY_NAME: &str = "llama-server";

/// Where the prebuilt engine releases come from. Load-bearing: this is the
/// real upstream project whose builds Kuro fetches, so it cannot be renamed
/// away without breaking model loading entirely.
const GITHUB_RELEASES: &str = "https://api.github.com/repos/ggml-org/llama.cpp/releases";

#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<ReleaseAssetJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseAssetJson {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// The archive to download for this host.
#[derive(Debug, Clone)]
pub struct ResolvedAsset {
    pub tag: String,
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// Substring identifying the release asset for the current platform.
///
/// V1 ships macOS only. Adding a platform means adding a branch here; nothing
/// else in the bootstrap path is platform-specific.
pub fn asset_pattern() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("bin-macos-arm64.tar.gz"),
        ("macos", "x86_64") => Ok("bin-macos-x64.tar.gz"),
        (os, arch) => Err(KuroError::engine(format!(
            "Kuro does not ship an inference engine for {os}/{arch} yet"
        ))),
    }
}

/// Look up a release and pick the asset for this machine.
///
/// `tag` of `None` uses the pinned default; `Some("latest")` resolves whatever
/// was published most recently.
pub async fn resolve_asset(client: &reqwest::Client, tag: Option<&str>) -> Result<ResolvedAsset> {
    let tag = tag.unwrap_or(DEFAULT_ENGINE_TAG);
    let url = if tag == "latest" {
        format!("{GITHUB_RELEASES}/latest")
    } else {
        format!("{GITHUB_RELEASES}/tags/{tag}")
    };

    let response = client.get(&url).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(KuroError::engine(format!(
            "engine release `{tag}` does not exist"
        )));
    }
    if !response.status().is_success() {
        return Err(KuroError::engine(format!(
            "could not reach GitHub to fetch the engine ({})",
            response.status()
        )));
    }

    let release: Release = response.json().await?;
    let pattern = asset_pattern()?;

    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(pattern))
        .ok_or_else(|| {
            KuroError::engine(format!(
                "engine release `{}` has no `{pattern}` build",
                release.tag_name
            ))
        })?;

    Ok(ResolvedAsset {
        tag: release.tag_name.clone(),
        name: asset.name.clone(),
        url: asset.browser_download_url.clone(),
        size: asset.size,
    })
}

/// Return a ready-to-run engine, downloading it if this machine does not have
/// it yet.
pub async fn ensure_engine(
    client: &reqwest::Client,
    db: &Db,
    paths: &Paths,
    tag: Option<&str>,
    on_progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
) -> Result<EngineRuntimeRecord> {
    let requested = tag.unwrap_or(DEFAULT_ENGINE_TAG);

    // A cached runtime is only usable if its binary is still on disk; the user
    // may have cleared Application Support.
    if let Some(existing) = db.get_engine_runtime(requested)? {
        if Path::new(&existing.path).exists() {
            return adopt_installed(db, existing);
        }
        db.delete_engine_runtime(requested)?;
    }

    let asset = resolve_asset(client, tag).await?;

    if let Some(existing) = db.get_engine_runtime(&asset.tag)? {
        if Path::new(&existing.path).exists() {
            return adopt_installed(db, existing);
        }
        db.delete_engine_runtime(&asset.tag)?;
    }

    let archive_path = paths.engine_downloads_dir().join(&asset.name);
    let outcome = download_to_file(
        client,
        &asset.url,
        &archive_path,
        None, // GitHub does not publish checksums for these assets.
        Arc::new(AtomicBool::new(false)),
        on_progress,
    )
    .await?;

    let version_dir = paths.engine_version_dir(&asset.tag);
    if version_dir.exists() {
        std::fs::remove_dir_all(&version_dir)?;
    }
    std::fs::create_dir_all(&version_dir)?;

    extract_archive(&archive_path, &version_dir)?;

    let extracted = find_engine_binary(&version_dir)?.ok_or_else(|| {
        KuroError::engine("the downloaded engine archive did not contain an engine executable")
    })?;
    let binary = adopt_binary(&extracted)?;

    make_executable(&binary)?;
    clear_quarantine(&binary);

    // The archive is large and no longer needed once extracted.
    let _ = std::fs::remove_file(&archive_path);

    let runtime = EngineRuntimeRecord {
        id: asset.tag.clone(),
        version: asset.tag.clone(),
        asset_name: asset.name.clone(),
        path: binary.to_string_lossy().to_string(),
        sha256: outcome.sha256,
        backend: if cfg!(target_os = "macos") { "metal" } else { "cpu" }.to_string(),
        downloaded_at: chrono::Utc::now().to_rfc3339(),
    };
    db.upsert_engine_runtime(&runtime)?;

    Ok(runtime)
}

/// Extract a `.tar.gz` release into `dest`.
///
/// Entries are inspected before extraction so a crafted archive cannot write
/// outside the destination directory.
fn extract_archive(archive: &Path, dest: &Path) -> Result<()> {
    let listing = std::process::Command::new("/usr/bin/tar")
        .arg("-tzf")
        .arg(archive)
        .output()?;

    if !listing.status.success() {
        return Err(KuroError::engine(
            "the downloaded engine archive could not be read",
        ));
    }

    let entries = String::from_utf8_lossy(&listing.stdout);
    for entry in entries.lines().filter(|line| !line.trim().is_empty()) {
        if !is_contained(dest, Path::new(entry)) {
            return Err(KuroError::engine(format!(
                "engine archive contains an unsafe path and was rejected: {entry}"
            )));
        }
    }

    let status = std::process::Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .arg("--no-same-owner")
        .status()?;

    if !status.success() {
        return Err(KuroError::engine("could not extract the engine archive"));
    }
    Ok(())
}

/// Locate the engine executable anywhere under `root`.
///
/// The archive puts it in a versioned subdirectory whose layout has changed
/// before, so it is searched for rather than assumed. Both the upstream name
/// and Kuro's own are accepted, so re-scanning a directory Kuro has already
/// adopted still finds it.
pub fn find_engine_binary(root: &Path) -> Result<Option<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];

    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                pending.push(path);
            } else if entry.file_name() == ENGINE_BINARY_NAME
                || entry.file_name() == UPSTREAM_BINARY_NAME
            {
                return Ok(Some(path));
            }
        }
    }

    Ok(None)
}

/// Bring an already-cached engine up to the current naming.
///
/// An install made before Kuro renamed its engine still has a record pointing
/// at the upstream filename. Renaming it here rather than only on download
/// means an existing install does not have to re-fetch the engine to stop
/// showing someone else's process name in the process list.
///
/// A failure is not fatal: the recorded binary is still perfectly runnable
/// under its old name, and refusing to start a model over a cosmetic rename
/// would be a poor trade.
pub fn adopt_installed(db: &Db, existing: EngineRuntimeRecord) -> Result<EngineRuntimeRecord> {
    let path = Path::new(&existing.path);
    if is_adopted(path) {
        return Ok(existing);
    }

    match adopt_binary(path) {
        Ok(adopted) => {
            let renamed = EngineRuntimeRecord {
                path: adopted.to_string_lossy().to_string(),
                ..existing
            };
            db.upsert_engine_runtime(&renamed)?;
            Ok(renamed)
        }
        Err(error) => {
            tracing::warn!(%error, "could not rename the installed engine; using it as it is");
            Ok(existing)
        }
    }
}

/// Directory the engine and its libraries are installed into, inside the
/// per-tag directory Kuro owns.
const ENGINE_DIR_NAME: &str = "engine";

/// Whether a recorded path already uses Kuro's own names throughout.
fn is_adopted(path: &Path) -> bool {
    let named = path.file_name().is_some_and(|name| name == ENGINE_BINARY_NAME);
    let housed = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == ENGINE_DIR_NAME);
    named && housed
}

/// Give the extracted engine Kuro's own names.
///
/// Two renames, both in place: the directory the archive unpacked into, and the
/// executable inside it. Neither moves anything across directories — the
/// executable resolves its sibling libraries through `@loader_path`, so it has
/// to keep them beside it — and nothing about the binary's behaviour depends on
/// either name. What they do determine is what the user sees: the process name
/// in Activity Monitor and `ps`, the path in a crash report, and the folder they
/// find if they ever open Kuro's data directory.
fn adopt_binary(extracted: &Path) -> Result<PathBuf> {
    let home = extracted
        .parent()
        .ok_or_else(|| KuroError::engine("the engine executable has no containing directory"))?;

    // Rename the directory first: doing it after would invalidate the path just
    // computed for the executable.
    let home = if home.file_name().is_some_and(|name| name == ENGINE_DIR_NAME) {
        home.to_path_buf()
    } else {
        let renamed = home.with_file_name(ENGINE_DIR_NAME);
        // A leftover from an interrupted install would make the rename fail.
        if renamed.exists() {
            let _ = std::fs::remove_dir_all(&renamed);
        }
        std::fs::rename(home, &renamed).map_err(|error| {
            KuroError::engine(format!("could not install the engine directory: {error}"))
        })?;
        renamed
    };

    let adopted = home.join(ENGINE_BINARY_NAME);
    if adopted.exists() {
        return Ok(adopted);
    }

    let current = home.join(UPSTREAM_BINARY_NAME);
    std::fs::rename(&current, &adopted).map_err(|error| {
        KuroError::engine(format!("could not install the engine executable: {error}"))
    })?;
    Ok(adopted)
}

fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

/// Best-effort removal of the quarantine flag.
///
/// Files fetched programmatically are not usually quarantined, so a failure
/// here is not worth failing the install over — the launch would surface it.
fn clear_quarantine(path: &Path) {
    let _ = std::process::Command::new("/usr/bin/xattr")
        .arg("-d")
        .arg("com.apple.quarantine")
        .arg(path)
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_asset_for_this_platform() {
        let pattern = asset_pattern().expect("macOS is supported");
        assert!(pattern.ends_with(".tar.gz"), "macOS releases ship as tarballs");
        assert!(pattern.contains("macos"));
    }

    #[test]
    fn finds_the_binary_at_any_depth() {
        let root = std::env::temp_dir().join(format!("kuro-find-{}", uuid::Uuid::new_v4()));
        let nested = root.join("engine-b10182").join("bin");
        std::fs::create_dir_all(&nested).expect("mkdir");
        std::fs::write(nested.join(UPSTREAM_BINARY_NAME), b"#!/bin/sh\n").expect("write");
        // A sibling executable the search must not mistake for the server.
        std::fs::write(nested.join("engine-cli"), b"#!/bin/sh\n").expect("write");

        let found = find_engine_binary(&root).expect("search").expect("present");
        assert!(found.ends_with(UPSTREAM_BINARY_NAME));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reports_a_missing_binary_rather_than_guessing() {
        let root = std::env::temp_dir().join(format!("kuro-empty-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("mkdir");

        assert!(find_engine_binary(&root).expect("search").is_none());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_installed_engine_runs_under_kuros_own_name() {
        // The rename is what makes the process list say `kuro-engine`, so it is
        // asserted rather than assumed.
        let root = std::env::temp_dir().join(format!("kuro-adopt-{}", uuid::Uuid::new_v4()));
        let unpacked = root.join("llama-b10182");
        std::fs::create_dir_all(&unpacked).expect("mkdir");
        let extracted = unpacked.join(UPSTREAM_BINARY_NAME);
        std::fs::write(&extracted, b"#!/bin/sh\n").expect("write");
        // A sibling library, which must still sit beside the executable
        // afterwards or the binary will not load.
        std::fs::write(unpacked.join("libggml-base.dylib"), b"stub").expect("write");

        let adopted = adopt_binary(&extracted).expect("adopt");

        assert!(adopted.ends_with(ENGINE_BINARY_NAME));
        assert!(adopted.exists(), "the renamed binary must be on disk");
        assert!(!extracted.exists(), "the upstream name must not be left behind");
        assert!(!unpacked.exists(), "the unpacked directory is renamed too");
        assert_eq!(
            adopted.parent().and_then(Path::file_name).map(|n| n.to_string_lossy()),
            Some(std::borrow::Cow::Borrowed(ENGINE_DIR_NAME)),
        );
        assert!(
            adopted.with_file_name("libggml-base.dylib").exists(),
            "the libraries must move with the executable, not away from it"
        );
        assert!(is_adopted(&adopted));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_engine_installed_before_the_rename_is_brought_forward() {
        // The upgrade path: a record written by an older build still points at
        // the upstream filename, and must not require a re-download to fix.
        let db = Db::open_in_memory().expect("open");
        let root = std::env::temp_dir().join(format!("kuro-upgrade-{}", uuid::Uuid::new_v4()));
        let unpacked = root.join("llama-b10182");
        std::fs::create_dir_all(&unpacked).expect("mkdir");
        let old = unpacked.join(UPSTREAM_BINARY_NAME);
        std::fs::write(&old, b"#!/bin/sh\n").expect("write");

        let record = EngineRuntimeRecord {
            id: "b10182".to_string(),
            version: "b10182".to_string(),
            asset_name: "engine-b10182-bin-macos-arm64.tar.gz".to_string(),
            path: old.to_string_lossy().to_string(),
            sha256: "aaa".to_string(),
            backend: "metal".to_string(),
            downloaded_at: chrono::Utc::now().to_rfc3339(),
        };
        db.upsert_engine_runtime(&record).expect("seed");

        let adopted = adopt_installed(&db, record).expect("adopt");

        assert!(adopted.path.ends_with(ENGINE_BINARY_NAME));
        assert!(!old.exists());
        let stored = db.get_engine_runtime("b10182").expect("get").expect("some");
        assert_eq!(stored.path, adopted.path, "the record must follow the rename");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn adopting_an_already_installed_binary_is_a_no_op() {
        let root = std::env::temp_dir().join(format!("kuro-readopt-{}", uuid::Uuid::new_v4()));
        let home = root.join(ENGINE_DIR_NAME);
        std::fs::create_dir_all(&home).expect("mkdir");
        let installed = home.join(ENGINE_BINARY_NAME);
        std::fs::write(&installed, b"#!/bin/sh\n").expect("write");

        assert!(is_adopted(&installed));
        assert_eq!(adopt_binary(&installed).expect("adopt"), installed);
        assert!(installed.exists());

        std::fs::remove_dir_all(&root).ok();
    }
}
