//! Remote model endpoints: API providers and rented hardware.
//!
//! One endpoint serves both because the mechanism is the same: an
//! OpenAI-compatible URL plus a key the user holds. Each entry carries a
//! `surface` (`provider` or `cloud`) so the UI can show them on two screens; see
//! [`kuro_core::cloud::presets::Surface`]. The storage table is still
//! `cloud_connectors`.

use axum::extract::{Path, State};
use axum::Json;
use kuro_core::cloud::presets::surface_for;
use kuro_core::cloud::PRESETS;
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// Connected providers and the presets on offer.
pub async fn list_providers(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let connected: Vec<Value> = state
        .db
        .list_cloud_connectors()?
        .into_iter()
        .map(|connector| {
            let has_key = state.providers.has_key(&connector);
            let mut encoded = serde_json::to_value(&connector).unwrap_or_else(|_| json!({}));
            // An internal detail that looks like a secret; not published.
            if let Some(object) = encoded.as_object_mut() {
                object.remove("keychain_ref");
            }
            encoded["hasKey"] = json!(has_key);
            // Screen this belongs on, derived from its preset so clients need not know the mapping.
            encoded["surface"] = json!(surface_for(&connector.provider).as_str());
            encoded
        })
        .collect();

    // `surface` is derived from `kind`, not stored, so one place decides an entry's screen.
    let presets: Vec<Value> = PRESETS
        .iter()
        .map(|preset| {
            let mut encoded = serde_json::to_value(preset).unwrap_or_else(|_| json!({}));
            encoded["surface"] = json!(preset.kind.surface().as_str());
            encoded
        })
        .collect();

    Ok(Json(json!({
        "providers": connected,
        "presets": presets,
    })))
}

#[derive(Debug, Deserialize)]
pub struct AddProviderRequest {
    /// A preset slug, or `custom`.
    pub provider: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, rename = "baseUrl")]
    pub base_url: Option<String>,
    #[serde(rename = "apiKey")]
    pub api_key: String,
}

/// Add a provider and immediately probe it.
pub async fn add_provider(
    State(state): State<SharedState>,
    Json(request): Json<AddProviderRequest>,
) -> AppResult<Json<Value>> {
    if request.api_key.trim().is_empty() {
        return Err(KuroError::bad_request("the API key is empty").into());
    }

    let connector = state
        .providers
        .add(
            &request.provider,
            request.label.as_deref(),
            request.base_url.as_deref(),
            &request.api_key,
        )
        .await?;

    // `add` already probed; the record carries the verdict, and a failed provider stays saved.
    Ok(Json(json!({
        "provider": {
            "id": connector.id,
            "provider": connector.provider,
            "label": connector.label,
            "base_url": connector.base_url,
            "status": connector.status,
            "last_error": connector.last_error,
            "models": connector.models,
            "enabled": connector.enabled,
            "hasKey": true,
        },
    })))
}

/// Re-probe a provider, refreshing its model list.
pub async fn test_provider(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    match state.providers.test(&id).await {
        Ok(models) => Ok(Json(json!({ "ok": true, "models": models }))),
        Err(error) => Ok(Json(json!({ "ok": false, "error": error.to_string() }))),
    }
}

#[derive(Debug, Deserialize)]
pub struct KeyRequest {
    #[serde(rename = "apiKey")]
    pub api_key: String,
}

pub async fn replace_key(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<KeyRequest>,
) -> AppResult<Json<Value>> {
    state.providers.replace_key(&id, &request.api_key).await?;

    let connector = state
        .db
        .get_cloud_connector(&id)?
        .ok_or_else(|| KuroError::not_found(format!("provider `{id}`")))?;

    Ok(Json(json!({
        "status": connector.status,
        "last_error": connector.last_error,
        "models": connector.models,
    })))
}

#[derive(Debug, Deserialize)]
pub struct EnabledRequest {
    pub enabled: bool,
}

pub async fn set_enabled(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<EnabledRequest>,
) -> AppResult<Json<Value>> {
    state.db.set_cloud_enabled(&id, request.enabled)?;
    Ok(Json(json!({ "enabled": request.enabled })))
}

pub async fn delete_provider(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let removed = state.providers.remove(&id)?;
    if !removed {
        return Err(KuroError::not_found(format!("provider `{id}`")).into());
    }
    Ok(Json(json!({ "deleted": true })))
}
