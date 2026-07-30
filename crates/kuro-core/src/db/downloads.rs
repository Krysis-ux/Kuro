//! Download progress tracking.
//!
//! Model weights and engine binaries share this table so that the UI can show
//! one unified list of in-flight transfers, and so a download interrupted by a
//! quit can be resumed on the next launch.

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{now, Db};
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadKind {
    Model,
    EngineBinary,
}

impl DownloadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::EngineBinary => "engine_binary",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "engine_binary" => Self::EngineBinary,
            _ => Self::Model,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Verifying,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "downloading" => Self::Downloading,
            "paused" => Self::Paused,
            "verifying" => Self::Verifying,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Queued,
        }
    }

    /// Whether this download still needs work; used to decide what to resume.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Downloading | Self::Paused | Self::Verifying
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadRecord {
    pub id: String,
    pub kind: DownloadKind,
    pub target_id: String,
    pub label: String,
    pub url: String,
    pub dest_path: String,
    pub total_bytes: Option<i64>,
    pub downloaded_bytes: i64,
    pub sha256_expected: Option<String>,
    pub status: DownloadStatus,
    pub error: Option<String>,
    pub started_at: String,
    pub updated_at: String,
}

fn download_from_row(row: &Row<'_>) -> rusqlite::Result<DownloadRecord> {
    let kind_raw: String = row.get("kind")?;
    let status_raw: String = row.get("status")?;
    Ok(DownloadRecord {
        id: row.get("id")?,
        kind: DownloadKind::parse(&kind_raw),
        target_id: row.get("target_id")?,
        label: row.get("label")?,
        url: row.get("url")?,
        dest_path: row.get("dest_path")?,
        total_bytes: row.get("total_bytes")?,
        downloaded_bytes: row.get("downloaded_bytes")?,
        sha256_expected: row.get("sha256_expected")?,
        status: DownloadStatus::parse(&status_raw),
        error: row.get("error")?,
        started_at: row.get("started_at")?,
        updated_at: row.get("updated_at")?,
    })
}

impl Db {
    #[allow(clippy::too_many_arguments)]
    pub fn create_download(
        &self,
        kind: DownloadKind,
        target_id: &str,
        label: &str,
        url: &str,
        dest_path: &str,
        total_bytes: Option<i64>,
    ) -> Result<DownloadRecord> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO downloads (
                     id, kind, target_id, label, url, dest_path, total_bytes,
                     downloaded_bytes, status, started_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 'queued', ?8, ?8)",
                params![
                    id,
                    kind.as_str(),
                    target_id,
                    label,
                    url,
                    dest_path,
                    total_bytes,
                    timestamp
                ],
            )?;
            Ok(())
        })?;

        self.get_download(&id)?
            .ok_or_else(|| crate::KuroError::other("download vanished after insert"))
    }

    pub fn get_download(&self, id: &str) -> Result<Option<DownloadRecord>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    "SELECT * FROM downloads WHERE id = ?1",
                    params![id],
                    download_from_row,
                )
                .optional()?;
            Ok(found)
        })
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadRecord>> {
        self.with(|conn| {
            let mut stmt =
                conn.prepare("SELECT * FROM downloads ORDER BY updated_at DESC LIMIT 100")?;
            let rows = stmt
                .query_map([], download_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// The still-running download for a target, if any.
    ///
    /// Prevents starting a second transfer for a model the user double-clicked.
    pub fn active_download_for_target(&self, target_id: &str) -> Result<Option<DownloadRecord>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    "SELECT * FROM downloads
                      WHERE target_id = ?1
                        AND status IN ('queued', 'downloading', 'paused', 'verifying')
                      ORDER BY updated_at DESC LIMIT 1",
                    params![target_id],
                    download_from_row,
                )
                .optional()?;
            Ok(found)
        })
    }

    pub fn update_download_progress(
        &self,
        id: &str,
        downloaded_bytes: i64,
        total_bytes: Option<i64>,
    ) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE downloads
                    SET downloaded_bytes = ?2,
                        total_bytes = COALESCE(?3, total_bytes),
                        status = 'downloading',
                        updated_at = ?4
                  WHERE id = ?1",
                params![id, downloaded_bytes, total_bytes, now()],
            )?;
            Ok(())
        })
    }

    pub fn set_download_status(
        &self,
        id: &str,
        status: DownloadStatus,
        error: Option<&str>,
    ) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE downloads SET status = ?2, error = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, status.as_str(), error, now()],
            )?;
            Ok(())
        })
    }

    pub fn delete_download(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_download(db: &Db, target: &str) -> DownloadRecord {
        db.create_download(
            DownloadKind::Model,
            target,
            "Qwen3 4B",
            "https://example.invalid/model.gguf",
            "/tmp/model.gguf",
            Some(1000),
        )
        .expect("create")
    }

    #[test]
    fn tracks_progress_and_completion() {
        let db = Db::open_in_memory().expect("open");
        let download = new_download(&db, "qwen3-4b:q4_k_m");
        assert_eq!(download.status, DownloadStatus::Queued);

        db.update_download_progress(&download.id, 500, None).expect("progress");
        let stored = db.get_download(&download.id).expect("get").expect("some");
        assert_eq!(stored.downloaded_bytes, 500);
        assert_eq!(stored.total_bytes, Some(1000));
        assert_eq!(stored.status, DownloadStatus::Downloading);

        db.set_download_status(&download.id, DownloadStatus::Completed, None)
            .expect("complete");
        let stored = db.get_download(&download.id).expect("get").expect("some");
        assert!(!stored.status.is_active());
    }

    #[test]
    fn finds_an_in_flight_download_but_ignores_finished_ones() {
        let db = Db::open_in_memory().expect("open");
        let download = new_download(&db, "qwen3-4b:q4_k_m");

        assert!(db
            .active_download_for_target("qwen3-4b:q4_k_m")
            .expect("lookup")
            .is_some());

        db.set_download_status(&download.id, DownloadStatus::Completed, None)
            .expect("complete");

        assert!(db
            .active_download_for_target("qwen3-4b:q4_k_m")
            .expect("lookup")
            .is_none());
    }

    #[test]
    fn records_failure_reason() {
        let db = Db::open_in_memory().expect("open");
        let download = new_download(&db, "qwen3-4b:q4_k_m");

        db.set_download_status(&download.id, DownloadStatus::Failed, Some("checksum mismatch"))
            .expect("fail");

        let stored = db.get_download(&download.id).expect("get").expect("some");
        assert_eq!(stored.status, DownloadStatus::Failed);
        assert_eq!(stored.error.as_deref(), Some("checksum mismatch"));
    }
}
