//! Deciding which engines are running.
//!
//! One `llama-server` per loaded model, started on demand and stopped when it
//! has been idle. Loads are serialised behind a single lock: two large models
//! loading at once would compete for the same memory, so making that impossible
//! is intentional rather than a limitation.
//!
//! Crash handling is deliberately simple. A supervisor task watches each child;
//! if it exits unexpectedly the entry is removed from the running set and the
//! failure is logged. The next request then starts a fresh engine through the
//! normal path. That gives transparent recovery with no possibility of a
//! restart loop.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::db::{Db, DownloadKind, DownloadStatus, ModelStatus};
use crate::engine::bootstrap::ensure_engine;
use crate::engine::port::allocate_port;
use crate::engine::process::{
    spawn_engine, tail_log, terminate, wait_until_healthy, EngineLaunchSpec,
};
use crate::hardware::HardwareInfo;
use crate::paths::Paths;
use crate::settings::{EngineSettings, KEY_ENGINE_RELEASE_TAG};
use crate::{http, KuroError, Result};

/// How often idle models are checked for unloading.
const IDLE_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

struct EngineHandle {
    port: u16,
    pid: u32,
    loaded_at: String,
    /// Unix seconds of the last request, used by the idle sweep.
    last_used: Arc<AtomicI64>,
}

/// A model currently held in memory.
#[derive(Debug, Clone, Serialize)]
pub struct LoadedEngine {
    pub model_id: String,
    pub port: u16,
    pub pid: u32,
    pub loaded_at: String,
    pub idle_seconds: i64,
}

pub struct EngineManager {
    db: Db,
    paths: Paths,
    hardware: HardwareInfo,
    outbound: reqwest::Client,
    loopback: reqwest::Client,
    engines: Arc<Mutex<HashMap<String, EngineHandle>>>,
}

impl EngineManager {
    pub fn new(db: Db, paths: Paths, hardware: HardwareInfo) -> Result<Self> {
        Ok(Self {
            db,
            paths,
            hardware,
            outbound: http::client()?,
            loopback: http::loopback_client()?,
            engines: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Client configured for talking to engine processes.
    pub fn loopback_client(&self) -> &reqwest::Client {
        &self.loopback
    }

    /// Base URL of a running engine, loading it first if necessary.
    pub async fn ensure_base_url(&self, model_id: &str) -> Result<String> {
        let port = self.ensure_loaded(model_id).await?;
        Ok(format!("http://127.0.0.1:{port}"))
    }

    /// Guarantee that `model_id` is loaded, and return its internal port.
    pub async fn ensure_loaded(&self, model_id: &str) -> Result<u16> {
        let mut engines = self.engines.lock().await;

        if let Some(handle) = engines.get(model_id) {
            handle.last_used.store(unix_now(), Ordering::Relaxed);
            return Ok(handle.port);
        }

        let model = self
            .db
            .get_model(model_id)?
            .ok_or_else(|| KuroError::not_found(format!("model `{model_id}`")))?;

        if model.status != ModelStatus::Ready {
            return Err(KuroError::bad_request(format!(
                "`{model_id}` is not ready to run (status: {})",
                model.status.as_str()
            )));
        }

        let model_path = model.file_path.clone().ok_or_else(|| {
            KuroError::model(format!("`{model_id}` has no weights file on disk"))
        })?;

        let runtime = self.ensure_engine_binary().await?;
        let settings = EngineSettings::resolve(&self.db, &self.hardware)?;

        let taken: Vec<u16> = engines.values().map(|handle| handle.port).collect();
        let port = allocate_port(&taken)?;

        let log_path = self.paths.engine_log_file(model_id);
        let spec = EngineLaunchSpec {
            binary: runtime.path.into(),
            model_path: model_path.into(),
            model_alias: model_id.to_string(),
            port,
            context_size: settings.context_size,
            gpu_layers: settings.gpu_layers,
            threads: settings.threads,
            log_path: log_path.clone(),
        };

        tracing::info!(model_id, port, "starting engine");
        let mut child = spawn_engine(&spec)?;
        let pid = child.id().unwrap_or_default();

        if let Err(error) = wait_until_healthy(&self.loopback, port, &log_path).await {
            // Do not leave a half-started process behind holding memory.
            terminate(&mut child).await;
            self.db.set_model_error(model_id, &error.to_string())?;
            return Err(error);
        }

        let last_used = Arc::new(AtomicI64::new(unix_now()));
        engines.insert(
            model_id.to_string(),
            EngineHandle {
                port,
                pid,
                loaded_at: chrono::Utc::now().to_rfc3339(),
                last_used: last_used.clone(),
            },
        );

        self.supervise(model_id.to_string(), pid, child, log_path);
        self.db.touch_model_used(model_id)?;
        tracing::info!(model_id, port, pid, "engine ready");

        Ok(port)
    }

    /// Watch a child process and clean up when it exits.
    fn supervise(
        &self,
        model_id: String,
        pid: u32,
        mut child: tokio::process::Child,
        log_path: std::path::PathBuf,
    ) {
        let engines = self.engines.clone();

        tokio::spawn(async move {
            let status = child.wait().await;
            let mut guard = engines.lock().await;

            // Only forget this engine if it is still the one we started; a
            // replacement may already have taken its place.
            let is_current = guard
                .get(&model_id)
                .map(|handle| handle.pid == pid)
                .unwrap_or(false);

            if is_current {
                guard.remove(&model_id);
                match status {
                    Ok(exit) if exit.success() => {
                        tracing::info!(model_id, "engine stopped");
                    }
                    Ok(exit) => {
                        tracing::error!(
                            model_id,
                            code = exit.code(),
                            "engine exited unexpectedly; it will restart on the next request. \
                             Recent output:\n{}",
                            tail_log(&log_path, 15)
                        );
                    }
                    Err(error) => {
                        tracing::error!(model_id, %error, "could not wait on engine process");
                    }
                }
            }
        });
    }

    /// Download the engine if this machine does not have it yet.
    async fn ensure_engine_binary(&self) -> Result<crate::db::EngineRuntimeRecord> {
        let tag = self
            .db
            .get_setting(KEY_ENGINE_RELEASE_TAG)?
            .and_then(|value| value.as_str().map(str::to_string));

        // Fast path: already installed, no download record needed.
        let requested = tag.as_deref().unwrap_or(crate::engine::bootstrap::DEFAULT_ENGINE_TAG);
        if let Some(existing) = self.db.get_engine_runtime(requested)? {
            if std::path::Path::new(&existing.path).exists() {
                return Ok(existing);
            }
        }

        tracing::info!("downloading inference engine");
        let record = self.db.create_download(
            DownloadKind::EngineBinary,
            requested,
            "Inference engine",
            "https://github.com/ggml-org/llama.cpp/releases",
            &self.paths.engine_versions_dir().to_string_lossy(),
            None,
        )?;

        let db = self.db.clone();
        let download_id = record.id.clone();
        let mut on_progress = move |downloaded: u64, total: Option<u64>| {
            let _ = db.update_download_progress(
                &download_id,
                downloaded as i64,
                total.map(|value| value as i64),
            );
        };

        let result = ensure_engine(
            &self.outbound,
            &self.db,
            &self.paths,
            tag.as_deref(),
            &mut on_progress,
        )
        .await;

        match &result {
            Ok(_) => {
                self.db
                    .set_download_status(&record.id, DownloadStatus::Completed, None)?;
            }
            Err(error) => {
                self.db.set_download_status(
                    &record.id,
                    DownloadStatus::Failed,
                    Some(&error.to_string()),
                )?;
            }
        }

        result
    }

    /// Record activity so an in-use model is not unloaded underneath a request.
    pub async fn touch(&self, model_id: &str) {
        let engines = self.engines.lock().await;
        if let Some(handle) = engines.get(model_id) {
            handle.last_used.store(unix_now(), Ordering::Relaxed);
        }
    }

    pub async fn loaded(&self) -> Vec<LoadedEngine> {
        let engines = self.engines.lock().await;
        let now = unix_now();

        let mut listed: Vec<LoadedEngine> = engines
            .iter()
            .map(|(model_id, handle)| LoadedEngine {
                model_id: model_id.clone(),
                port: handle.port,
                pid: handle.pid,
                loaded_at: handle.loaded_at.clone(),
                idle_seconds: (now - handle.last_used.load(Ordering::Relaxed)).max(0),
            })
            .collect();

        listed.sort_by(|a, b| a.model_id.cmp(&b.model_id));
        listed
    }

    pub async fn is_loaded(&self, model_id: &str) -> bool {
        self.engines.lock().await.contains_key(model_id)
    }

    /// Stop an engine. Returns whether one was running.
    pub async fn unload(&self, model_id: &str) -> Result<bool> {
        let handle = {
            let mut engines = self.engines.lock().await;
            engines.remove(model_id)
        };

        let Some(handle) = handle else {
            return Ok(false);
        };

        tracing::info!(model_id, pid = handle.pid, "stopping engine");
        // The supervisor owns the Child, so signal the process directly. It
        // observes the exit and completes the cleanup.
        signal_terminate(handle.pid);
        Ok(true)
    }

    pub async fn unload_all(&self) {
        let handles: Vec<(String, u32)> = {
            let mut engines = self.engines.lock().await;
            engines.drain().map(|(id, handle)| (id, handle.pid)).collect()
        };

        for (model_id, pid) in handles {
            tracing::info!(model_id, pid, "stopping engine");
            signal_terminate(pid);
        }
    }

    /// Unload models that have been idle longer than the configured timeout.
    pub async fn idle_sweep(&self) {
        let settings = match EngineSettings::resolve(&self.db, &self.hardware) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "could not read idle-unload setting");
                return;
            }
        };

        if settings.idle_unload_minutes == 0 {
            return;
        }

        let threshold = i64::from(settings.idle_unload_minutes) * 60;
        let now = unix_now();

        let stale: Vec<String> = {
            let engines = self.engines.lock().await;
            engines
                .iter()
                .filter(|(_, handle)| now - handle.last_used.load(Ordering::Relaxed) > threshold)
                .map(|(model_id, _)| model_id.clone())
                .collect()
        };

        for model_id in stale {
            tracing::info!(model_id, "unloading idle model");
            let _ = self.unload(&model_id).await;
        }
    }

    /// Run the idle sweep on a timer until `shutdown` is set.
    pub async fn run_idle_loop(self: Arc<Self>, shutdown: Arc<AtomicBool>) {
        let mut ticker = tokio::time::interval(IDLE_SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            if shutdown.load(Ordering::Relaxed) {
                return;
            }
            self.idle_sweep().await;
        }
    }
}

fn signal_terminate(pid: u32) {
    if pid == 0 {
        return;
    }
    // SAFETY: sending SIGTERM to a pid this process spawned.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
}

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ModelSource, NewModel};
    use crate::hardware;

    fn manager() -> (EngineManager, Db) {
        let db = Db::open_in_memory().expect("db");
        let paths = Paths {
            root: std::env::temp_dir().join(format!("kuro-mgr-{}", uuid::Uuid::new_v4())),
        };
        paths.create_all().expect("dirs");
        let manager =
            EngineManager::new(db.clone(), paths, hardware::detect()).expect("manager");
        (manager, db)
    }

    #[tokio::test]
    async fn refuses_to_load_an_unknown_model() {
        let (manager, _db) = manager();
        let error = manager.ensure_loaded("does-not-exist").await.unwrap_err();
        assert!(matches!(error, KuroError::NotFound(_)));
    }

    #[tokio::test]
    async fn refuses_to_load_a_model_that_is_still_downloading() {
        let (manager, db) = manager();
        db.upsert_model(&NewModel {
            id: "half-pulled".to_string(),
            display_name: "Half Pulled".to_string(),
            source: ModelSource::Curated,
            hf_repo: None,
            hf_file: None,
            quant: None,
            param_count: None,
            family: None,
            capabilities: vec![],
            context_length: None,
            file_size_bytes: None,
        })
        .expect("insert");

        let error = manager.ensure_loaded("half-pulled").await.unwrap_err();

        assert!(matches!(error, KuroError::BadRequest(_)), "got {error:?}");
        assert!(error.to_string().contains("not ready"));
    }

    #[tokio::test]
    async fn nothing_is_loaded_on_a_fresh_manager() {
        let (manager, _db) = manager();
        assert!(manager.loaded().await.is_empty());
        assert!(!manager.is_loaded("anything").await);
        assert!(!manager.unload("anything").await.expect("unload"));
    }
}
