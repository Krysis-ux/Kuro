//! Turning "I want this model" into weights on disk.
//!
//! A pull is split into a plan and an execution. Planning is cheap and
//! network-light, so callers (the CLI, the API, the Models page) can show the
//! user exactly what is about to be downloaded — repository, file, size — and
//! catch problems like a gated repository before any bytes move.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::db::{Db, DownloadKind, DownloadStatus, ModelSource, NewModel};
use crate::paths::Paths;
use crate::{KuroError, Result};

use super::curated::find_curated;
use super::download::download_to_file;
use super::hf::{
    choose_gguf, list_gguf_files, parse_model_ref, quant_from_filename, resolve_download_url,
    ModelRef,
};

/// Everything needed to fetch one model, resolved against the source.
#[derive(Debug, Clone)]
pub struct PullPlan {
    pub model_id: String,
    pub display_name: String,
    pub source: ModelSource,
    pub repo: String,
    pub filename: String,
    pub url: String,
    pub size_bytes: u64,
    /// Published by Hugging Face for LFS files; used to verify the download.
    pub sha256: Option<String>,
    pub quant: Option<String>,
    pub param_count: Option<String>,
    pub family: Option<String>,
    pub capabilities: Vec<String>,
    pub context_length: Option<i64>,
}

/// Resolve a user-supplied reference into a concrete download.
pub async fn plan_pull(client: &reqwest::Client, reference: &str) -> Result<PullPlan> {
    let parsed = parse_model_ref(reference)?;

    let (repo, requested_quant, curated) = match &parsed {
        ModelRef::Curated { slug, quant } => {
            let curated = find_curated(slug).ok_or_else(|| {
                KuroError::not_found(format!(
                    "`{slug}` is not in the recommended list. Pass a Hugging Face repository \
                     like `owner/repo` to pull anything else."
                ))
            })?;
            let quant = quant
                .clone()
                .unwrap_or_else(|| curated.default_quant.to_string());
            (curated.hf_repo.to_string(), Some(quant), Some(curated))
        }
        ModelRef::HuggingFace { repo, quant } => (repo.clone(), quant.clone(), None),
    };

    let files = list_gguf_files(client, &repo).await?;
    let chosen = choose_gguf(&files, requested_quant.as_deref())?;

    let quant = quant_from_filename(&chosen.filename).or(requested_quant);

    let (model_id, display_name, source) = match curated {
        Some(curated) => (
            curated.model_id(quant.as_deref().unwrap_or(curated.default_quant)),
            curated.display_name.to_string(),
            ModelSource::Curated,
        ),
        None => {
            let base = repo_slug(&repo);
            let id = match &quant {
                Some(quant) => format!("{base}:{}", quant.to_ascii_lowercase()),
                None => base.clone(),
            };
            (id, repo.clone(), ModelSource::Huggingface)
        }
    };

    Ok(PullPlan {
        model_id,
        display_name,
        source,
        repo: repo.clone(),
        url: resolve_download_url(&repo, &chosen.filename),
        filename: chosen.filename,
        size_bytes: chosen.size,
        sha256: chosen.sha256,
        quant,
        param_count: curated.map(|c| c.param_count.to_string()),
        family: curated.map(|c| c.family.to_string()),
        capabilities: curated
            .map(|c| c.capabilities.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default(),
        context_length: curated.map(|c| i64::from(c.context_length)),
    })
}

/// Register the model and its download row without transferring anything yet.
///
/// Splitting this from [`run_pull`] lets the HTTP layer answer immediately with
/// a download id the client can poll, while the bytes move in the background.
pub fn prepare_pull(db: &Db, paths: &Paths, plan: &PullPlan) -> Result<PreparedPull> {
    if let Some(existing) = db.active_download_for_target(&plan.model_id)? {
        return Err(KuroError::bad_request(format!(
            "`{}` is already downloading ({}%)",
            plan.model_id,
            percent(existing.downloaded_bytes, existing.total_bytes)
        )));
    }

    let destination = paths.model_file(&plan.model_id, &plan.filename);

    db.upsert_model(&NewModel {
        id: plan.model_id.clone(),
        display_name: plan.display_name.clone(),
        source: plan.source,
        hf_repo: Some(plan.repo.clone()),
        hf_file: Some(plan.filename.clone()),
        quant: plan.quant.clone(),
        param_count: plan.param_count.clone(),
        family: plan.family.clone(),
        capabilities: plan.capabilities.clone(),
        context_length: plan.context_length,
        file_size_bytes: Some(plan.size_bytes as i64),
    })?;

    let record = db.create_download(
        DownloadKind::Model,
        &plan.model_id,
        &plan.display_name,
        &plan.url,
        &destination.to_string_lossy(),
        Some(plan.size_bytes as i64),
    )?;

    Ok(PreparedPull {
        download_id: record.id,
        destination: destination.to_string_lossy().to_string(),
    })
}

#[derive(Debug, Clone)]
pub struct PreparedPull {
    pub download_id: String,
    pub destination: String,
}

/// Transfer the weights for an already-prepared pull.
///
/// A failure leaves an explanatory message on both the download and the model
/// row rather than a silently missing file.
pub async fn run_pull(
    client: &reqwest::Client,
    db: &Db,
    plan: &PullPlan,
    prepared: &PreparedPull,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let destination = std::path::Path::new(&prepared.destination);

    let progress_db = db.clone();
    let progress_id = prepared.download_id.clone();
    let mut on_progress = move |downloaded: u64, total: Option<u64>| {
        let _ = progress_db.update_download_progress(
            &progress_id,
            downloaded as i64,
            total.map(|value| value as i64),
        );
    };

    let outcome = download_to_file(
        client,
        &plan.url,
        destination,
        plan.sha256.as_deref(),
        cancel,
        &mut on_progress,
    )
    .await;

    match outcome {
        Ok(outcome) => {
            db.set_download_status(&prepared.download_id, DownloadStatus::Completed, None)?;
            db.set_model_ready(
                &plan.model_id,
                &prepared.destination,
                outcome.bytes as i64,
                &outcome.sha256,
            )?;
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            db.set_download_status(
                &prepared.download_id,
                DownloadStatus::Failed,
                Some(&message),
            )?;
            db.set_model_error(&plan.model_id, &message)?;
            Err(error)
        }
    }
}

/// Prepare and run a pull in one call, for callers that block until it is done.
pub async fn execute_pull(
    client: &reqwest::Client,
    db: &Db,
    paths: &Paths,
    plan: &PullPlan,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let prepared = prepare_pull(db, paths, plan)?;
    run_pull(client, db, plan, &prepared, cancel).await
}

/// Remove a model's weights and its registration.
pub fn remove_model(db: &Db, paths: &Paths, model_id: &str) -> Result<()> {
    let model = db
        .get_model(model_id)?
        .ok_or_else(|| KuroError::not_found(format!("model `{model_id}`")))?;

    if let Some(path) = &model.file_path {
        let _ = std::fs::remove_file(path);
    }

    // Also drop the per-model directory, which holds any leftover `.part` file
    // from an interrupted download.
    let model_dir = paths.model_file(model_id, "x");
    if let Some(parent) = model_dir.parent() {
        if parent.starts_with(paths.models_dir()) {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    db.delete_model(model_id)?;
    Ok(())
}

/// Last path segment of a repository, normalised for use as a model id.
fn repo_slug(repo: &str) -> String {
    let name = repo.rsplit('/').next().unwrap_or(repo).to_ascii_lowercase();
    // Nearly every GGUF repository ends in `-gguf`, which adds nothing to the id.
    name.strip_suffix("-gguf").unwrap_or(&name).to_string()
}

fn percent(done: i64, total: Option<i64>) -> i64 {
    match total {
        Some(total) if total > 0 => (done * 100 / total).clamp(0, 100),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_a_readable_id_from_a_repository() {
        assert_eq!(repo_slug("unsloth/Qwen3-4B-Instruct-2507-GGUF"), "qwen3-4b-instruct-2507");
        assert_eq!(repo_slug("bartowski/Phi-4-GGUF"), "phi-4");
        assert_eq!(repo_slug("owner/PlainModel"), "plainmodel");
    }

    #[test]
    fn percent_handles_unknown_totals() {
        assert_eq!(percent(50, Some(100)), 50);
        assert_eq!(percent(50, None), 0);
        assert_eq!(percent(50, Some(0)), 0);
        assert_eq!(percent(500, Some(100)), 100, "must never exceed 100");
    }

    #[test]
    fn removing_an_unknown_model_is_an_error_not_a_silent_success() {
        let db = Db::open_in_memory().expect("db");
        let paths = Paths {
            root: std::env::temp_dir().join(format!("kuro-rm-{}", uuid::Uuid::new_v4())),
        };
        let error = remove_model(&db, &paths, "nope").unwrap_err();
        assert!(matches!(error, KuroError::NotFound(_)));
    }
}
