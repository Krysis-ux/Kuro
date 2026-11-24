//! The Kuro LLM daemon.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use kuro_core::cloud::ProviderRegistry;
use kuro_core::db::Db;
use kuro_core::engine::EngineManager;
use kuro_core::mcp::McpManager;
use kuro_core::{hardware, http, tools, Paths, SecretStore};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

mod error;
mod routes;
mod state;
mod static_files;

use state::AppState;

/// Default port, chosen to avoid the ones other local model servers use (8080, 11434, 1234).
const DEFAULT_PORT: u16 = 8420;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("KURO_LOG")
                .unwrap_or_else(|_| "kuro_server=info,kuro_core=info,warn".into()),
        )
        .with_target(false)
        .init();

    let paths = Paths::resolve_and_create().context("preparing the Kuro data directory")?;
    let db = Db::open(&paths.database_file()).context("opening the Kuro database")?;

    // The models directory can be moved to another disk. Read once here so an
    // unwritable directory fails at startup, not several gigabytes into a download.
    let paths = match db
        .get_setting(kuro_core::settings::KEY_MODELS_DIRECTORY)
        .ok()
        .flatten()
        .and_then(|value| value.as_str().map(str::to_string))
        .filter(|dir| !dir.trim().is_empty())
    {
        Some(dir) => paths
            .with_models_dir(dir.trim())
            .context("preparing the models directory")?,
        None => paths,
    };
    let hardware = hardware::detect();

    // A duplicate built-in tool name is a programming error; fail at startup, not mid-conversation.
    tools::assert_builtin_names_are_unique().context("checking the built-in tools")?;

    let port = resolve_port();
    let engines = Arc::new(
        EngineManager::new(db.clone(), paths.clone(), hardware.clone())
            .context("starting the engine manager")?,
    );

    let outbound = http::client()?;
    let secrets = SecretStore::new(paths.credentials_file());
    let mcp = Arc::new(McpManager::new(
        db.clone(),
        secrets.clone(),
        outbound.clone(),
    ));
    let providers = Arc::new(ProviderRegistry::new(
        db.clone(),
        secrets.clone(),
        outbound.clone(),
    ));

    // Kept in memory because it is read on a path that must not touch the database.
    let free = kuro_core::free::FreePool::new();
    free.set_allow_keyless(kuro_core::settings::allow_keyless(&db).unwrap_or(true));
    // Restore each provider's last-known catalogue so the picker is populated after a restart.
    free.restore_catalogues(routes::free::stored_catalogues(&db));
    // Restore which providers were refusing, so a rejected key is not retried as if available.
    free.restore_troubles(routes::free::stored_troubles(&db));
    // Load user-added skills before the first request so the `/` palette includes them.
    match kuro_core::skills::custom::reload(&db) {
        Ok(0) => {}
        Ok(count) => tracing::info!(count, "loaded skills you added"),
        Err(error) => tracing::warn!(%error, "could not load your own skills"),
    }
    // And user-written subagents, so the first coding turn can delegate to them.
    match kuro_core::agents::reload(&db) {
        Ok(0) => {}
        Ok(count) => tracing::info!(count, "loaded subagents you wrote"),
        Err(error) => tracing::warn!(%error, "could not load your own subagents"),
    }

    let app_state = Arc::new(AppState {
        db,
        paths: paths.clone(),
        hardware,
        engines: engines.clone(),
        outbound,
        secrets,
        mcp,
        providers,
        free,
        processes: kuro_core::workspace::ProcessRegistry::new(),
        started_at: chrono::Utc::now(),
        port,
        download_cancels: Mutex::new(HashMap::new()),
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    tokio::spawn(engines.clone().run_idle_loop(shutdown.clone()));
    tokio::spawn(routes::automations::run_scheduler(app_state.clone()));

    // Loopback only: there is no authentication yet, so the server must not be
    // reachable from the network. LAN serving arrives with API keys.
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = bind(address, port).await?;

    tracing::info!("Kuro LLM listening on http://127.0.0.1:{port}");
    tracing::info!("data directory: {}", paths.root.display());

    let app = routes::router(app_state);
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;

    // Engines are child processes; stop them or they keep holding gigabytes of memory.
    shutdown.store(true, Ordering::Relaxed);
    tracing::info!("stopping engines");
    engines.unload_all().await;

    result.context("server error")
}

/// Set by the outgoing process on its successor so a restart waits for the port instead of failing.
pub const RESTART_MARKER: &str = "KURO_RESTARTING";

/// How long a restarting successor waits for the old process to release the
/// port. Generous, since shutdown stops the engine processes.
const RESTART_BIND_TIMEOUT: Duration = Duration::from_secs(20);

/// Take the listening socket.
async fn bind(address: SocketAddr, port: u16) -> anyhow::Result<TcpListener> {
    let restarting = std::env::var_os(RESTART_MARKER).is_some();

    if !restarting {
        return TcpListener::bind(address).await.with_context(|| {
            format!("could not bind to port {port}. Another program may be using it.")
        });
    }

    let deadline = std::time::Instant::now() + RESTART_BIND_TIMEOUT;
    let mut last_error = None;

    while std::time::Instant::now() < deadline {
        match TcpListener::bind(address).await {
            Ok(listener) => return Ok(listener),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }

    Err(anyhow::anyhow!(
        "restarting, but port {port} was still held after {} seconds: {}",
        RESTART_BIND_TIMEOUT.as_secs(),
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ))
}

fn resolve_port() -> u16 {
    std::env::var("KURO_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => {}
        _ = terminate => {}
    }

    tracing::info!("shutting down");
}
