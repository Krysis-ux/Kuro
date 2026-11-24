use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use kuro_core::KuroError;

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
/// Replies first and shuts down after a short delay, because a request that
/// dies with the server it is killing looks like a crash to the browser.
/// Engines are stopped first so their processes (gigabytes of memory) do not
/// outlive the daemon.
pub async fn shutdown(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let loaded = state.engines.loaded().await.len();
    tracing::info!(engines = loaded, "shutdown requested from the interface");

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        state.engines.unload_all().await;
        tracing::info!("stopping");
        // `main`'s graceful-shutdown future listens for SIGTERM: same path as Ctrl-C.
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

/// Relaunch the daemon, for when a restart is the fix: an engine setting that
/// applies on load, a wedged child process, a new build on disk.
///
/// The successor starts before this process exits, with a marker telling it to
/// wait for the port instead of failing. Exiting first would turn a restart
/// into a shutdown.
pub async fn restart(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let executable = std::env::current_exe()
        .map_err(|error| KuroError::other(format!("cannot find Kuro's own binary: {error}")))?;

    // Carry over the arguments so the successor gets the same flags.
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    let mut command = std::process::Command::new(&executable);
    command
        .args(&arguments)
        .env(crate::RESTART_MARKER, "1")
        // Own process group (below), so the signal that stops us does not stop the successor.
        .stdin(std::process::Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let child = command
        .spawn()
        .map_err(|error| KuroError::other(format!("could not start the new process: {error}")))?;

    tracing::info!(pid = child.id(), "restarting; successor started");

    let port = state.port;
    tokio::spawn(async move {
        // Give this response time to reach the browser so the page can poll for the successor.
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        state.engines.unload_all().await;
        tracing::info!("handing over");

        #[cfg(unix)]
        // SAFETY: raising SIGTERM in our own process, which `main` handles.
        unsafe {
            libc::raise(libc::SIGTERM);
        }
        #[cfg(not(unix))]
        std::process::exit(0);
    });

    Ok(Json(json!({
        "restarting": true,
        "port": port,
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
