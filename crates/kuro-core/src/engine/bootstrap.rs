//! Obtaining the `llama-server` binary.
//!
//! Kuro does not build llama.cpp. On first use it downloads the prebuilt
//! release that matches this machine, extracts it into a per-tag directory and
//! records it, so later launches reuse the cached copy.
//!
//! The release tag is pinned to one that has been tested rather than tracking
//! `latest`, because llama.cpp's asset naming changes from time to time and a
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

/// llama.cpp release Kuro ships against. Verified to run on macOS arm64.
pub const DEFAULT_ENGINE_TAG: &str = "b10182";

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
/// llama.cpp published most recently.
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
            "llama.cpp release `{tag}` does not exist"
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
                "llama.cpp release `{}` has no `{pattern}` build",
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
            return Ok(existing);
        }
        db.delete_engine_runtime(requested)?;
    }

    let asset = resolve_asset(client, tag).await?;

    if let Some(existing) = db.get_engine_runtime(&asset.tag)? {
        if Path::new(&existing.path).exists() {
            return Ok(existing);
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

    let binary = find_llama_server(&version_dir)?.ok_or_else(|| {
        KuroError::engine("the downloaded engine archive did not contain `llama-server`")
    })?;

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

/// Locate `llama-server` anywhere under `root`.
///
/// The archive currently puts it in a `llama-<tag>/` directory, but that layout
/// has changed before, so it is searched for rather than assumed.
pub fn find_llama_server(root: &Path) -> Result<Option<PathBuf>> {
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
            } else if entry.file_name() == "llama-server" {
                return Ok(Some(path));
            }
        }
    }

    Ok(None)
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
        let nested = root.join("llama-b10182").join("bin");
        std::fs::create_dir_all(&nested).expect("mkdir");
        std::fs::write(nested.join("llama-server"), b"#!/bin/sh\n").expect("write");
        std::fs::write(nested.join("llama-cli"), b"#!/bin/sh\n").expect("write");

        let found = find_llama_server(&root).expect("search").expect("present");
        assert!(found.ends_with("llama-server"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reports_a_missing_binary_rather_than_guessing() {
        let root = std::env::temp_dir().join(format!("kuro-empty-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("mkdir");

        assert!(find_llama_server(&root).expect("search").is_none());

        std::fs::remove_dir_all(&root).ok();
    }
}
