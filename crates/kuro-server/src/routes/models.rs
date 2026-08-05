use axum::extract::{Path, Query, State};
use axum::Json;
use kuro_core::catalog::{self, search, CURATED_MODELS};
use kuro_core::free;
use kuro_core::hardware::estimate_fit;
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Search Hugging Face for GGUF repositories.
///
/// With no query this returns the most-downloaded ones, which makes the same
/// endpoint serve both "browse" and "search" — the distinction is not one a user
/// thinks in.
pub async fn search_hub(
    State(state): State<SharedState>,
    Query(query): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    let limit = query.limit.unwrap_or_else(search::default_limit);
    let term = query.q.as_deref().map(str::trim).unwrap_or("");

    let hits = if term.is_empty() {
        search::trending_gguf(&state.outbound, limit).await?
    } else {
        search::search_gguf(&state.outbound, term, limit).await?
    };

    let installed = state.db.list_models()?;

    let described: Vec<Value> = hits
        .into_iter()
        .map(|hit| {
            let fit = search::estimate_fit(&hit, &state.hardware);
            // Any installed model from this repository counts as installed, since
            // the quantization is chosen at download time.
            let is_installed = installed
                .iter()
                .any(|model| model.hf_repo.as_deref() == Some(hit.repo.as_str()));

            let mut encoded = serde_json::to_value(&hit).unwrap_or_else(|_| json!({}));
            encoded["fit"] = serde_json::to_value(fit).unwrap_or(Value::Null);
            encoded["installed"] = json!(is_installed);
            encoded
        })
        .collect();

    Ok(Json(json!({ "models": described })))
}

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

            // The same classifier the provider catalogues go through. It was
            // only ever applied to those, which left an embedding model sitting
            // in the local list as an ordinary choice — and picking one does
            // not fail, it answers. `qwen3-embedding-0.6b` asked "hi" replies
            // with several hundred repetitions of the word "lines", because an
            // embedding model handed a chat prompt produces whatever the
            // decoder makes of a vector.
            let classified = kuro_core::classify::classify(&model.id);

            json!({
                "model": model,
                "loaded": is_loaded,
                "fit": fit,
                "kind": classified.kind.as_str(),
                // False for embedding models, rerankers and the rest. The
                // picker greys these rather than hiding them: a model you
                // downloaded and cannot find is a worse puzzle than one that
                // says why it is not for this.
                "chat": classified.kind.is_chat(),
            })
        })
        .collect();

    // Provider models ride along in the same response so the composer needs one
    // request to render a picker containing everything the user can talk to.
    let mut remote = state.providers.remote_models()?;
    remote.extend(free_models(&state));

    Ok(Json(json!({
        "models": described,
        "remote": remote,
    })))
}

/// The free pool's entries for the model picker.
///
/// Only flavours that can actually be served appear. A picker offering
/// "Kuro Free · coding" to somebody with nothing behind it is offering a model
/// that answers every message with an error, and the place to explain that is
/// the free-models screen rather than four dead rows in a dropdown.
///
/// "Can be served" is asked of the pool rather than of the key list, because
/// those stopped being the same question when shared endpoints arrived: with
/// them switched on there are no keys and the pool can still answer, and an
/// early return on an empty key list would have made the whole tier invisible.
fn free_models(state: &SharedState) -> Vec<kuro_core::cloud::RemoteModel> {
    let keys = crate::routes::free::stored_keys(state);

    // Opening the picker is the natural moment to find out what these keys
    // reach. Without this the provider sections are empty until something else
    // happens to read the catalogues — a message sent, or a visit to the free
    // screen — so the first look at a newly pasted key showed a provider with
    // no models under it and no indication that was temporary.
    //
    // In the background, so the picker still opens instantly. The list arrives
    // on the next poll, which is the right trade for a menu.
    crate::routes::free::refresh_catalogues_in_background(state, &keys);

    let mut out: Vec<kuro_core::cloud::RemoteModel> = free::FreeFlavour::ALL
        .iter()
        .filter(|flavour| state.free.choose(**flavour, &keys).is_some())
        .map(|flavour| kuro_core::cloud::RemoteModel {
            id: flavour.model_id(),
            name: flavour.label().to_string(),
            // One group in the picker, whichever provider happens to answer.
            // Grouping by the provider would make the same model appear to move
            // between headings as allowances ran out.
            connector_id: "kuro-free".to_string(),
            connector_label: "Kuro Free".to_string(),
            provider: "free".to_string(),
            // Every one of these routes across providers, so marking one as
            // pooled and the rest not would be a distinction without a
            // difference. The group heading already says what they are.
            pooled: false,
            pool_size: 0,
            free: true,
            specialities: flavour.speciality().map(|s| vec![s.label()]).unwrap_or_default(),
            params_b: None,
            // A pooled row is only listed when something can serve it, so by
            // construction it is available. That check is the `filter` above.
            unavailable: None,
            fix_url: None,
        })
        .collect();

    out.extend(named_free_models(state, &keys));
    out
}

/// Why a model cannot be picked, in words rather than a status code.
///
/// A picker row saying "429" is a row nobody can act on. Each of these names
/// the thing that has to change and, where it is not obvious, who has to change
/// it — an exhausted allowance is waiting, a rejected key is retyping, and an
/// unprovisioned model is a page to visit.
fn explain(trouble: free::Trouble, provider: &str) -> String {
    match trouble {
        free::Trouble::RateLimited => {
            format!("{provider} is out of allowance for now — it comes back")
        }
        free::Trouble::Rejected => {
            format!("{provider} rejected the key. Check it on the Free models screen")
        }
        // Not "this model was retired". NVIDIA answers 404 for a model the key
        // was never provisioned for, which is by far the commoner reason to see
        // this, and the two are indistinguishable from the response.
        free::Trouble::Gone => {
            format!("{provider} would not serve this model on your key")
        }
    }
}

/// Each free provider's own catalogue, named individually.
///
/// The four pooled rows above answer "just give me something that works". This
/// answers the other question, which the picker could not previously ask at
/// all: *which* models does the key I pasted actually reach? Somebody who adds
/// an NVIDIA key gets a catalogue of sixty-odd models, and until now every one
/// of them was invisible — the pool would silently pick one and the picker
/// showed none.
///
/// Grouped under the provider rather than under Kuro Free, because these are a
/// choice of endpoint rather than a preference handed to the pool: picking one
/// here pins the request to that provider.
fn named_free_models(
    state: &SharedState,
    keys: &std::collections::HashMap<String, String>,
) -> Vec<kuro_core::cloud::RemoteModel> {
    let allow_keyless = state.free.allows_keyless();
    let mut out = Vec::new();

    for provider in free::FREE_PROVIDERS {
        if !provider.is_reachable(keys, allow_keyless) {
            continue;
        }
        let group = kuro_core::cloud::RemoteGroup {
            connector_id: &free::connector_id(provider.slug),
            connector_label: provider.name,
            provider: provider.slug,
        };

        // Why the whole provider is out, if it is. Applies to every one of its
        // models, so it is asked once.
        let provider_trouble = state.free.trouble(provider.slug);

        for model in state.free.advertised_chat_models(provider) {
            let mut row = kuro_core::cloud::RemoteModel::described(
                format!("{}{}/{model}", free::MODEL_PREFIX, provider.slug),
                group,
                &model,
                // Everything reachable here is inside the provider's free
                // allowance by construction: `advertised_chat_models` has
                // already applied the marker that separates the allowance from
                // the bill.
                true,
            );

            // The model's own refusal wins over the provider's, because it is
            // the more specific fact and the one with a remedy attached.
            let trouble = state
                .free
                .model_trouble(provider.slug, &model)
                .or(provider_trouble);

            if let Some(trouble) = trouble {
                row.unavailable = Some(explain(trouble, provider.name));
                if trouble.stale_catalogue() {
                    row.fix_url = provider
                        .model_key_url
                        .map(|template| template.replace("{model}", &model));
                }
            }

            out.push(row);
        }
    }

    out
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
                "purposes": curated.purposes,
                // The heading this model is filed under when it is shown once.
                "primaryPurpose": catalog::curated::primary_purpose(curated).as_str(),
                "contextLength": curated.context_length,
                "approxSizeBytes": curated.approx_size_bytes,
                "blurb": curated.blurb,
                "installed": existing.is_some(),
                "status": existing.map(|model| model.status),
                "fit": estimate_fit(curated.approx_size_bytes, &state.hardware),
            })
        })
        .collect();

    Ok(Json(json!({
        "models": described,
        // The headings travel with the list, so the screen does not keep its own
        // copy of what the purposes are or what order they belong in.
        "purposes": catalog::curated::Purpose::ALL
            .iter()
            .map(|purpose| json!({
                "id": purpose.as_str(),
                "label": purpose.label(),
                "blurb": purpose.blurb(),
            }))
            .collect::<Vec<_>>(),
    })))
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
