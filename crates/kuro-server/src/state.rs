use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use kuro_core::db::Db;
use kuro_core::engine::EngineManager;
use kuro_core::hardware::HardwareInfo;
use kuro_core::Paths;
use tokio::sync::Mutex;

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub db: Db,
    pub paths: Paths,
    pub hardware: HardwareInfo,
    pub engines: Arc<EngineManager>,
    /// Client for Hugging Face and GitHub.
    pub outbound: reqwest::Client,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub port: u16,
    /// Cancellation flags for in-flight downloads, keyed by download id.
    pub download_cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn uptime_seconds(&self) -> i64 {
        (chrono::Utc::now() - self.started_at).num_seconds().max(0)
    }

    /// Register a cancellation flag so `POST /downloads/{id}/cancel` can stop
    /// a transfer that is already streaming.
    pub async fn register_download(&self, download_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.download_cancels
            .lock()
            .await
            .insert(download_id.to_string(), flag.clone());
        flag
    }

    pub async fn cancel_download(&self, download_id: &str) -> bool {
        let flags = self.download_cancels.lock().await;
        match flags.get(download_id) {
            Some(flag) => {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    pub async fn forget_download(&self, download_id: &str) {
        self.download_cancels.lock().await.remove(download_id);
    }
}
