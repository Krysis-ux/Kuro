//! OpenAI-compatible endpoints.
//!
//! These are a thin proxy in front of the engine rather than a reimplementation:
//! whatever `llama-server` supports — tool calls, JSON schemas, vision content
//! blocks, sampling parameters Kuro has never heard of — passes through
//! untouched. Kuro's contribution is resolving the model name and making sure
//! an engine is running before the request arrives.
//!
//! Existing OpenAI SDK code works by pointing `base_url` at Kuro.

use axum::body::Body;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use kuro_core::db::ModelStatus;
use kuro_core::KuroError;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::routes::common::resolve_model_id;
use crate::state::SharedState;

pub async fn chat_completions(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> AppResult<Response> {
    proxy(state, body, "/v1/chat/completions").await
}

pub async fn completions(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> AppResult<Response> {
    proxy(state, body, "/v1/completions").await
}

pub async fn embeddings(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> AppResult<Response> {
    proxy(state, body, "/v1/embeddings").await
}

/// Forward a request to the engine that serves the requested model.
///
/// The response body is streamed rather than buffered, so token-by-token
/// streaming reaches the caller with no added latency.
async fn proxy(state: SharedState, mut body: Value, path: &str) -> AppResult<Response> {
    let requested = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);
    let model_id = resolve_model_id(&state, requested.as_deref()).await?;

    // Rewrite the model field to the resolved id so that clients which passed
    // `auto`, or nothing at all, get a response naming a real model.
    if let Value::Object(fields) = &mut body {
        fields.insert("model".to_string(), json!(model_id));
    }

    let base_url = state.engines.ensure_base_url(&model_id).await?;
    let upstream = state
        .engines
        .loopback_client()
        .post(format!("{base_url}{path}"))
        .json(&body)
        .send()
        .await?;

    state.engines.touch(&model_id).await;

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = upstream.headers().get(CONTENT_TYPE).cloned();

    let mut builder = Response::builder().status(status);
    if let Some(content_type) = content_type {
        builder = builder.header(CONTENT_TYPE, content_type);
    }

    builder
        .header("cache-control", "no-cache")
        .body(Body::from_stream(upstream.bytes_stream()))
        .map_err(|error| AppError(KuroError::other(error)))
}

/// Installed models in OpenAI's list shape.
///
/// Only ready models are listed: advertising one that is still downloading
/// would make an SDK fail at request time instead of at selection time.
pub async fn list_models(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let models: Vec<Value> = state
        .db
        .list_models()?
        .into_iter()
        .filter(|model| model.status == ModelStatus::Ready)
        .map(|model| {
            json!({
                "id": model.id,
                "object": "model",
                "created": to_unix(&model.added_at),
                "owned_by": "kuro",
            })
        })
        .collect();

    Ok(Json(json!({ "object": "list", "data": models })))
}

fn to_unix(rfc3339: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|parsed| parsed.timestamp())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_stored_timestamps() {
        assert_eq!(to_unix("2026-01-01T00:00:00+00:00"), 1_767_225_600);
    }

    #[test]
    fn unparseable_timestamps_do_not_panic() {
        assert_eq!(to_unix("not a date"), 0);
    }
}
