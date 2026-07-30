use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// Liveness probe used by the CLI to find a running daemon.
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// What the Settings → Server page shows.
pub async fn status(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let loaded = state.engines.loaded().await;

    Ok(Json(json!({
        "name": "Kuro LLM",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "running",
        "host": "127.0.0.1",
        "port": state.port,
        "address": format!("http://127.0.0.1:{}", state.port),
        "uptimeSeconds": state.uptime_seconds(),
        "startedAt": state.started_at.to_rfc3339(),
        "dataDirectory": state.paths.root.to_string_lossy(),
        "loadedModels": loaded,
    })))
}

/// Detected hardware plus the engine defaults derived from it.
pub async fn hardware(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let settings =
        kuro_core::settings::EngineSettings::resolve(&state.db, &state.hardware)?;

    Ok(Json(json!({
        "hardware": state.hardware,
        "effectiveEngineSettings": settings,
    })))
}
