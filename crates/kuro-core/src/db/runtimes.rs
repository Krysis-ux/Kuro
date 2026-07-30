//! Cached `llama-server` builds.
//!
//! One row per llama.cpp release tag that has been downloaded and extracted, so
//! Kuro can reuse an engine across restarts instead of re-fetching it.

use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use super::{now, Db};
use crate::Result;

#[derive(Debug, Clone, Serialize)]
pub struct EngineRuntimeRecord {
    /// llama.cpp release tag, e.g. `b7891`.
    pub id: String,
    pub version: String,
    pub asset_name: String,
    /// Absolute path to the extracted `llama-server` executable.
    pub path: String,
    pub sha256: String,
    /// `metal` or `cpu`.
    pub backend: String,
    pub downloaded_at: String,
}

fn runtime_from_row(row: &Row<'_>) -> rusqlite::Result<EngineRuntimeRecord> {
    Ok(EngineRuntimeRecord {
        id: row.get("id")?,
        version: row.get("version")?,
        asset_name: row.get("asset_name")?,
        path: row.get("path")?,
        sha256: row.get("sha256")?,
        backend: row.get("backend")?,
        downloaded_at: row.get("downloaded_at")?,
    })
}

impl Db {
    pub fn upsert_engine_runtime(&self, runtime: &EngineRuntimeRecord) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "INSERT INTO engine_runtimes (
                     id, version, asset_name, path, sha256, backend, downloaded_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (id) DO UPDATE SET
                     version       = excluded.version,
                     asset_name    = excluded.asset_name,
                     path          = excluded.path,
                     sha256        = excluded.sha256,
                     backend       = excluded.backend,
                     downloaded_at = excluded.downloaded_at",
                params![
                    runtime.id,
                    runtime.version,
                    runtime.asset_name,
                    runtime.path,
                    runtime.sha256,
                    runtime.backend,
                    now(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_engine_runtime(&self, id: &str) -> Result<Option<EngineRuntimeRecord>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    "SELECT * FROM engine_runtimes WHERE id = ?1",
                    params![id],
                    runtime_from_row,
                )
                .optional()?;
            Ok(found)
        })
    }

    pub fn list_engine_runtimes(&self) -> Result<Vec<EngineRuntimeRecord>> {
        self.with(|conn| {
            let mut stmt =
                conn.prepare("SELECT * FROM engine_runtimes ORDER BY downloaded_at DESC")?;
            let rows = stmt
                .query_map([], runtime_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn delete_engine_runtime(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM engine_runtimes WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upserting_the_same_tag_keeps_one_row() {
        let db = Db::open_in_memory().expect("open");
        let mut runtime = EngineRuntimeRecord {
            id: "b7891".to_string(),
            version: "b7891".to_string(),
            asset_name: "llama-b7891-bin-macos-arm64.zip".to_string(),
            path: "/tmp/engine/b7891/llama-server".to_string(),
            sha256: "aaa".to_string(),
            backend: "metal".to_string(),
            downloaded_at: now(),
        };
        db.upsert_engine_runtime(&runtime).expect("insert");

        runtime.path = "/tmp/engine/b7891/bin/llama-server".to_string();
        db.upsert_engine_runtime(&runtime).expect("update");

        let all = db.list_engine_runtimes().expect("list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].path, "/tmp/engine/b7891/bin/llama-server");
    }
}
