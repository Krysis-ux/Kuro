//! Storage for remote model providers.
//!
//! One row per endpoint the user has connected — OpenRouter, OpenAI, Anthropic,
//! a rented GPU box, any OpenAI-compatible URL. The API key is not here; only
//! the reference into the credential store is, so this table can be read,
//! copied or attached to a bug report without leaking anything.

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{json_or, now, Db};
use crate::{KuroError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudStatus {
    /// Saved but never successfully reached.
    Untested,
    Ok,
    Error,
}

impl CloudStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Untested => "untested",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "ok" => Self::Ok,
            "error" => Self::Error,
            _ => Self::Untested,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudConnectorRecord {
    pub id: String,
    /// Preset slug (`openrouter`, `anthropic`, …) or `custom`.
    pub provider: String,
    pub label: String,
    /// Reference into the credential store. Never the key itself.
    pub keychain_ref: String,
    pub base_url: String,
    pub status: CloudStatus,
    pub last_tested_at: Option<String>,
    pub last_error: Option<String>,
    pub enabled: bool,
    /// Model ids from the last successful probe.
    pub models: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewCloudConnector {
    pub provider: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Required for `custom`; for a preset it overrides the preset's default.
    #[serde(default)]
    pub base_url: Option<String>,
    /// The key, held only long enough to move it into the credential store.
    #[serde(default)]
    pub api_key: Option<String>,
}

impl Db {
    pub fn list_cloud_connectors(&self) -> Result<Vec<CloudConnectorRecord>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS} FROM cloud_connectors ORDER BY created_at"
            ))?;
            let rows = stmt
                .query_map([], read_connector)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_cloud_connector(&self, id: &str) -> Result<Option<CloudConnectorRecord>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    &format!("SELECT {COLUMNS} FROM cloud_connectors WHERE id = ?1"),
                    params![id],
                    read_connector,
                )
                .optional()?)
        })
    }

    /// Insert a provider. The caller has already written the key to the
    /// credential store and passes the reference it used.
    pub fn insert_cloud_connector(
        &self,
        provider: &str,
        label: &str,
        base_url: &str,
        keychain_ref: &str,
    ) -> Result<CloudConnectorRecord> {
        let label = label.trim();
        let base_url = base_url.trim().trim_end_matches('/');
        if label.is_empty() {
            return Err(KuroError::bad_request("the provider needs a name"));
        }
        if base_url.is_empty() {
            return Err(KuroError::bad_request("the provider needs a base URL"));
        }

        let id = uuid::Uuid::new_v4().to_string();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO cloud_connectors
                     (id, provider, label, keychain_ref, base_url, status,
                      last_tested_at, last_error, created_at, enabled, models)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'untested', NULL, NULL, ?6, 1, '[]')",
                params![id, provider.trim(), label, keychain_ref, base_url, now()],
            )?;
            Ok(())
        })?;

        self.get_cloud_connector(&id)?
            .ok_or_else(|| KuroError::other("the provider disappeared immediately after insert"))
    }

    /// Record a successful probe together with the models it reported.
    pub fn set_cloud_ok(&self, id: &str, models: &[String]) -> Result<()> {
        let encoded = serde_json::to_string(models)?;
        self.with(|conn| {
            conn.execute(
                "UPDATE cloud_connectors
                    SET status = 'ok', models = ?2, last_tested_at = ?3, last_error = NULL
                  WHERE id = ?1",
                params![id, encoded, now()],
            )?;
            Ok(())
        })
    }

    /// Record a failed probe. The previously known model list is kept: a
    /// transient outage should not empty the picker.
    pub fn set_cloud_error(&self, id: &str, error: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE cloud_connectors
                    SET status = 'error', last_tested_at = ?2, last_error = ?3
                  WHERE id = ?1",
                params![id, now(), error],
            )?;
            Ok(())
        })
    }

    pub fn set_cloud_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        self.with(|conn| {
            let changed = conn.execute(
                "UPDATE cloud_connectors SET enabled = ?2 WHERE id = ?1",
                params![id, enabled as i64],
            )?;
            if changed == 0 {
                return Err(KuroError::not_found(format!("provider `{id}`")));
            }
            Ok(())
        })
    }

    pub fn delete_cloud_connector(&self, id: &str) -> Result<bool> {
        self.with(|conn| {
            let removed = conn.execute("DELETE FROM cloud_connectors WHERE id = ?1", params![id])?;
            Ok(removed > 0)
        })
    }
}

const COLUMNS: &str = "id, provider, label, keychain_ref, base_url, status, \
                       last_tested_at, last_error, created_at, enabled, models";

fn read_connector(row: &Row<'_>) -> rusqlite::Result<CloudConnectorRecord> {
    let status: String = row.get(5)?;
    let models: Option<String> = row.get(10)?;

    Ok(CloudConnectorRecord {
        id: row.get(0)?,
        provider: row.get(1)?,
        label: row.get(2)?,
        keychain_ref: row.get(3)?,
        base_url: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        status: CloudStatus::parse(&status),
        last_tested_at: row.get(6)?,
        last_error: row.get(7)?,
        created_at: row.get(8)?,
        enabled: row.get::<_, i64>(9)? != 0,
        models: json_or(models.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(db: &Db) -> CloudConnectorRecord {
        db.insert_cloud_connector(
            "openrouter",
            "OpenRouter",
            "https://openrouter.ai/api/v1/",
            "provider:openrouter:1",
        )
        .expect("insert")
    }

    #[test]
    fn inserts_and_normalises_the_base_url() {
        let db = Db::open_in_memory().expect("open");
        let created = insert(&db);

        assert_eq!(
            created.base_url, "https://openrouter.ai/api/v1",
            "a trailing slash would double up when paths are appended"
        );
        assert_eq!(created.status, CloudStatus::Untested);
        assert!(created.enabled);
        assert!(created.models.is_empty());
    }

    #[test]
    fn a_successful_probe_stores_the_model_list() {
        let db = Db::open_in_memory().expect("open");
        let created = insert(&db);

        db.set_cloud_ok(&created.id, &["anthropic/claude-opus-5".to_string()])
            .expect("ok");

        let reloaded = db.get_cloud_connector(&created.id).expect("get").expect("present");
        assert_eq!(reloaded.status, CloudStatus::Ok);
        assert_eq!(reloaded.models, vec!["anthropic/claude-opus-5".to_string()]);
        assert!(reloaded.last_tested_at.is_some());
        assert_eq!(reloaded.last_error, None);
    }

    #[test]
    fn a_failed_probe_keeps_the_last_known_models() {
        let db = Db::open_in_memory().expect("open");
        let created = insert(&db);
        db.set_cloud_ok(&created.id, &["gpt-4o-mini".to_string()]).expect("ok");

        db.set_cloud_error(&created.id, "401 unauthorized").expect("error");

        let reloaded = db.get_cloud_connector(&created.id).expect("get").expect("present");
        assert_eq!(reloaded.status, CloudStatus::Error);
        assert_eq!(reloaded.last_error.as_deref(), Some("401 unauthorized"));
        assert_eq!(
            reloaded.models,
            vec!["gpt-4o-mini".to_string()],
            "a transient outage must not empty the model picker"
        );
    }

    #[test]
    fn rejects_a_provider_without_a_url_or_label() {
        let db = Db::open_in_memory().expect("open");
        assert!(db.insert_cloud_connector("custom", "Box", "  ", "ref").is_err());
        assert!(db
            .insert_cloud_connector("custom", " ", "https://example.com", "ref")
            .is_err());
    }

    #[test]
    fn disabling_and_deleting_behave_predictably() {
        let db = Db::open_in_memory().expect("open");
        let created = insert(&db);

        db.set_cloud_enabled(&created.id, false).expect("disable");
        assert!(!db
            .get_cloud_connector(&created.id)
            .expect("get")
            .expect("present")
            .enabled);

        assert!(db.set_cloud_enabled("missing", true).is_err());
        assert!(db.delete_cloud_connector(&created.id).expect("delete"));
        assert!(!db.delete_cloud_connector(&created.id).expect("again"));
    }
}
