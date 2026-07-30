use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{now, Db};
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    /// Came from Kuro's built-in recommended list.
    Curated,
    /// Pulled from an arbitrary Hugging Face repo the user pasted.
    Huggingface,
    /// Imported from a file already on disk.
    Local,
}

impl ModelSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Curated => "curated",
            Self::Huggingface => "huggingface",
            Self::Local => "local",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "curated" => Self::Curated,
            "local" => Self::Local,
            _ => Self::Huggingface,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    Downloading,
    Ready,
    Error,
}

impl ModelStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Downloading => "downloading",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "ready" => Self::Ready,
            "error" => Self::Error,
            _ => Self::Downloading,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelRecord {
    pub id: String,
    pub display_name: String,
    pub source: ModelSource,
    pub hf_repo: Option<String>,
    pub hf_file: Option<String>,
    pub quant: Option<String>,
    pub param_count: Option<String>,
    pub family: Option<String>,
    pub capabilities: Vec<String>,
    pub context_length: Option<i64>,
    pub file_path: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub status: ModelStatus,
    pub error: Option<String>,
    pub added_at: String,
    pub last_used_at: Option<String>,
}

/// Fields required to register a model before its weights exist on disk.
#[derive(Debug, Clone)]
pub struct NewModel {
    pub id: String,
    pub display_name: String,
    pub source: ModelSource,
    pub hf_repo: Option<String>,
    pub hf_file: Option<String>,
    pub quant: Option<String>,
    pub param_count: Option<String>,
    pub family: Option<String>,
    pub capabilities: Vec<String>,
    pub context_length: Option<i64>,
    pub file_size_bytes: Option<i64>,
}

fn model_from_row(row: &Row<'_>) -> rusqlite::Result<ModelRecord> {
    let capabilities_raw: String = row.get("capabilities")?;
    let source_raw: String = row.get("source")?;
    let status_raw: String = row.get("status")?;

    Ok(ModelRecord {
        id: row.get("id")?,
        display_name: row.get("display_name")?,
        source: ModelSource::parse(&source_raw),
        hf_repo: row.get("hf_repo")?,
        hf_file: row.get("hf_file")?,
        quant: row.get("quant")?,
        param_count: row.get("param_count")?,
        family: row.get("family")?,
        capabilities: serde_json::from_str(&capabilities_raw).unwrap_or_default(),
        context_length: row.get("context_length")?,
        file_path: row.get("file_path")?,
        file_size_bytes: row.get("file_size_bytes")?,
        sha256: row.get("sha256")?,
        status: ModelStatus::parse(&status_raw),
        error: row.get("error")?,
        added_at: row.get("added_at")?,
        last_used_at: row.get("last_used_at")?,
    })
}

impl Db {
    /// Register a model, replacing any previous row with the same id.
    ///
    /// Re-pulling a model the user already has is a normal action (for example
    /// after deleting a corrupt file), so this is an upsert rather than an
    /// error.
    pub fn upsert_model(&self, model: &NewModel) -> Result<()> {
        let capabilities = serde_json::to_string(&model.capabilities)?;
        self.with(|conn| {
            conn.execute(
                "INSERT INTO models (
                     id, display_name, source, hf_repo, hf_file, quant, param_count,
                     family, capabilities, context_length, file_size_bytes, status, added_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'downloading', ?12)
                 ON CONFLICT (id) DO UPDATE SET
                     display_name    = excluded.display_name,
                     source          = excluded.source,
                     hf_repo         = excluded.hf_repo,
                     hf_file         = excluded.hf_file,
                     quant           = excluded.quant,
                     param_count     = excluded.param_count,
                     family          = excluded.family,
                     capabilities    = excluded.capabilities,
                     context_length  = excluded.context_length,
                     file_size_bytes = excluded.file_size_bytes,
                     status          = 'downloading',
                     error           = NULL",
                params![
                    model.id,
                    model.display_name,
                    model.source.as_str(),
                    model.hf_repo,
                    model.hf_file,
                    model.quant,
                    model.param_count,
                    model.family,
                    capabilities,
                    model.context_length,
                    model.file_size_bytes,
                    now(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_model(&self, id: &str) -> Result<Option<ModelRecord>> {
        self.with(|conn| {
            let found = conn
                .query_row("SELECT * FROM models WHERE id = ?1", params![id], |row| {
                    model_from_row(row)
                })
                .optional()?;
            Ok(found)
        })
    }

    pub fn list_models(&self) -> Result<Vec<ModelRecord>> {
        self.with(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM models ORDER BY added_at DESC")?;
            let rows = stmt
                .query_map([], model_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Mark a model usable and record where its weights ended up.
    pub fn set_model_ready(
        &self,
        id: &str,
        file_path: &str,
        file_size_bytes: i64,
        sha256: &str,
    ) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE models
                    SET status = 'ready', file_path = ?2, file_size_bytes = ?3,
                        sha256 = ?4, error = NULL
                  WHERE id = ?1",
                params![id, file_path, file_size_bytes, sha256],
            )?;
            Ok(())
        })
    }

    pub fn set_model_error(&self, id: &str, error: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE models SET status = 'error', error = ?2 WHERE id = ?1",
                params![id, error],
            )?;
            Ok(())
        })
    }

    pub fn touch_model_used(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE models SET last_used_at = ?2 WHERE id = ?1",
                params![id, now()],
            )?;
            Ok(())
        })
    }

    pub fn delete_model(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM models WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> NewModel {
        NewModel {
            id: id.to_string(),
            display_name: "Qwen3 4B Instruct".to_string(),
            source: ModelSource::Curated,
            hf_repo: Some("unsloth/Qwen3-4B-Instruct-2507-GGUF".to_string()),
            hf_file: Some("Qwen3-4B-Instruct-2507-Q4_K_M.gguf".to_string()),
            quant: Some("Q4_K_M".to_string()),
            param_count: Some("4B".to_string()),
            family: Some("qwen3".to_string()),
            capabilities: vec!["tools".to_string()],
            context_length: Some(32768),
            file_size_bytes: Some(2_500_000_000),
        }
    }

    #[test]
    fn round_trips_a_model_through_its_lifecycle() {
        let db = Db::open_in_memory().expect("open");
        db.upsert_model(&sample("qwen3-4b:q4_k_m")).expect("insert");

        let stored = db.get_model("qwen3-4b:q4_k_m").expect("get").expect("some");
        assert_eq!(stored.status, ModelStatus::Downloading);
        assert_eq!(stored.capabilities, vec!["tools".to_string()]);
        assert_eq!(stored.source, ModelSource::Curated);

        db.set_model_ready("qwen3-4b:q4_k_m", "/tmp/model.gguf", 2_500_000_000, "abc123")
            .expect("ready");
        let stored = db.get_model("qwen3-4b:q4_k_m").expect("get").expect("some");
        assert_eq!(stored.status, ModelStatus::Ready);
        assert_eq!(stored.file_path.as_deref(), Some("/tmp/model.gguf"));

        db.delete_model("qwen3-4b:q4_k_m").expect("delete");
        assert!(db.get_model("qwen3-4b:q4_k_m").expect("get").is_none());
    }

    #[test]
    fn re_pulling_a_model_resets_it_to_downloading() {
        let db = Db::open_in_memory().expect("open");
        db.upsert_model(&sample("qwen3-4b:q4_k_m")).expect("insert");
        db.set_model_error("qwen3-4b:q4_k_m", "disk full").expect("error");

        db.upsert_model(&sample("qwen3-4b:q4_k_m")).expect("re-insert");

        let stored = db.get_model("qwen3-4b:q4_k_m").expect("get").expect("some");
        assert_eq!(stored.status, ModelStatus::Downloading);
        assert!(stored.error.is_none(), "stale error must be cleared");
        assert_eq!(db.list_models().expect("list").len(), 1);
    }
}
