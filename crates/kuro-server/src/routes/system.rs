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

/// Stop the daemon.
///
/// The response is sent before the shutdown begins, because a request that dies
/// with the server it is killing looks like a crash to the browser. The delay is
/// long enough for the response to be flushed and short enough not to feel like a
/// hang.
///
/// Engines are stopped first. They are separate processes holding gigabytes, and
/// letting them outlive the daemon that supervises them would leave a machine with
/// no way to reclaim that memory short of finding the pids by hand.
pub async fn shutdown(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let loaded = state.engines.loaded().await.len();
    tracing::info!(engines = loaded, "shutdown requested from the interface");

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        state.engines.unload_all().await;
        tracing::info!("stopping");
        // The graceful-shutdown future in `main` is watching for a signal, so
        // raising one here takes the same path as Ctrl-C rather than a second,
        // separately-maintained exit route.
        #[cfg(unix)]
        // SAFETY: raising SIGTERM in our own process, which `main` handles.
        unsafe {
            libc::raise(libc::SIGTERM);
        }
        #[cfg(not(unix))]
        std::process::exit(0);
    });

    Ok(Json(json!({
        "stopping": true,
        "unloadingEngines": loaded,
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
