use axum::extract::{Path, State};
use axum::Json;
use kuro_core::catalog::{self, CURATED_MODELS};
use kuro_core::hardware::estimate_fit;
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// Models the user has pulled, each annotated with whether it is loaded and how
/// well it fits this machine.
pub async fn list_models(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let loaded = state.engines.loaded().await;
    let models = state.db.list_models()?;

    let described: Vec<Value> = models
        .into_iter()
        .map(|model| {
            let is_loaded = loaded.iter().any(|engine| engine.model_id == model.id);
            let fit = model
                .file_size_bytes
                .map(|bytes| estimate_fit(bytes as u64, &state.hardware));

            json!({
                "model": model,
                "loaded": is_loaded,
                "fit": fit,
            })
        })
        .collect();

    Ok(Json(json!({ "models": described })))
}

/// Kuro's built-in suggestions, with a fit estimate and whether each is already
/// installed.
pub async fn recommended_models(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let installed = state.db.list_models()?;

    let described: Vec<Value> = CURATED_MODELS
        .iter()
        .map(|curated| {
            let model_id = curated.model_id(curated.default_quant);
            let existing = installed.iter().find(|model| model.id == model_id);

            json!({
                "id": model_id,
                "slug": curated.slug,
                "displayName": curated.display_name,
                "repo": curated.hf_repo,
                "defaultQuant": curated.default_quant,
                "quants": curated.quants,
                "paramCount": curated.param_count,
                "family": curated.family,
                "capabilities": curated.capabilities,
                "contextLength": curated.context_length,
                "approxSizeBytes": curated.approx_size_bytes,
                "blurb": curated.blurb,
                "installed": existing.is_some(),
                "status": existing.map(|model| model.status),
                "fit": estimate_fit(curated.approx_size_bytes, &state.hardware),
            })
        })
        .collect();

    Ok(Json(json!({ "models": described })))
}

pub async fn get_model(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let model = state
        .db
        .get_model(&id)?
        .ok_or_else(|| KuroError::not_found(format!("model `{id}`")))?;

    let fit = model
        .file_size_bytes
        .map(|bytes| estimate_fit(bytes as u64, &state.hardware));

    Ok(Json(json!({
        "model": model,
        "loaded": state.engines.is_loaded(&id).await,
        "fit": fit,
    })))
}

#[derive(Debug, Deserialize)]
pub struct PullRequest {
    /// A curated slug (`qwen3-4b`), a repository (`owner/repo`), or a pasted
    /// Hugging Face URL. An optional `:QUANT` suffix picks a quantization.
    pub model: String,
}

/// Start a download and return immediately with something to poll.
pub async fn pull_model(
    State(state): State<SharedState>,
    Json(request): Json<PullRequest>,
) -> AppResult<Json<Value>> {
    let plan = catalog::plan_pull(&state.outbound, &request.model).await?;
    let prepared = catalog::prepare_pull(&state.db, &state.paths, &plan)?;
    let cancel = state.register_download(&prepared.download_id).await;

    let background = state.clone();
    let plan_for_task = plan.clone();
    let prepared_for_task = prepared.clone();
    tokio::spawn(async move {
        let result = catalog::run_pull(
            &background.outbound,
            &background.db,
            &plan_for_task,
            &prepared_for_task,
            cancel,
        )
        .await;

        if let Err(error) = result {
            tracing::warn!(model = %plan_for_task.model_id, %error, "pull failed");
        } else {
            tracing::info!(model = %plan_for_task.model_id, "pull complete");
        }
        background.forget_download(&prepared_for_task.download_id).await;
    });

    Ok(Json(json!({
        "downloadId": prepared.download_id,
        "model": {
            "id": plan.model_id,
            "displayName": plan.display_name,
            "repo": plan.repo,
            "file": plan.filename,
            "quant": plan.quant,
            "sizeBytes": plan.size_bytes,
        },
        "fit": estimate_fit(plan.size_bytes, &state.hardware),
    })))
}

/// Resolve a reference without downloading, so the UI can preview a pull.
pub async fn preview_pull(
    State(state): State<SharedState>,
    Json(request): Json<PullRequest>,
) -> AppResult<Json<Value>> {
    let plan = catalog::plan_pull(&state.outbound, &request.model).await?;

    Ok(Json(json!({
        "id": plan.model_id,
        "displayName": plan.display_name,
        "repo": plan.repo,
        "file": plan.filename,
        "quant": plan.quant,
        "sizeBytes": plan.size_bytes,
        "verifiable": plan.sha256.is_some(),
        "fit": estimate_fit(plan.size_bytes, &state.hardware),
    })))
}

pub async fn delete_model(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    // Stop it first; deleting weights out from under a running engine would
    // leave a process serving a file that no longer exists.
    state.engines.unload(&id).await?;
    catalog::remove_model(&state.db, &state.paths, &id)?;

    Ok(Json(json!({ "deleted": true })))
}

pub async fn load_model(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let port = state.engines.ensure_loaded(&id).await?;
    Ok(Json(json!({ "loaded": true, "modelId": id, "port": port })))
}

pub async fn unload_model(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let was_loaded = state.engines.unload(&id).await?;
    Ok(Json(json!({ "unloaded": was_loaded })))
}

/// Models currently held in memory, for `kuro ps` and Settings → Server.
pub async fn loaded_models(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    Ok(Json(json!({ "loaded": state.engines.loaded().await })))
}
