//! SQLite storage.
//!
//! Kuro is a single-user local application, so it uses one connection behind a
//! mutex rather than a pool. Every query here is a small indexed read or write
//! against a local file, so the lock is held for microseconds and is never held
//! across an `.await`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::{KuroError, Result};

mod cloud;
mod conversations;
mod downloads;
mod kv;
mod mcp;
mod memories;
mod models;
mod runtimes;

pub use cloud::{CloudConnectorRecord, CloudStatus, NewCloudConnector};
pub use conversations::{Conversation, Message, MessageCompletion, NewMessage};
pub use downloads::{DownloadKind, DownloadRecord, DownloadStatus};
pub use mcp::{McpServerRecord, McpStatus, McpTransport, NewMcpServer};
pub use memories::MemoryRecord;
pub use models::{ModelRecord, ModelSource, ModelStatus, NewModel};
pub use runtimes::EngineRuntimeRecord;

const SCHEMA: &str = include_str!("schema.sql");
const SCHEMA_VERSION: i32 = 2;

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// In-memory database, used by tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        // WAL keeps reads from blocking the writer, which matters while a large
        // download is checkpointing progress during an active chat.
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.with(|conn| {
            let current: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            if current >= SCHEMA_VERSION {
                return Ok(());
            }

            // Every statement in the schema is `IF NOT EXISTS`, so replaying the
            // whole file brings an older database up to date for anything that
            // is a new table or index.
            conn.execute_batch(SCHEMA)?;

            // Columns added to a table that already shipped. `IF NOT EXISTS` is
            // not available for `ALTER TABLE`, so each one is checked first
            // rather than adding it and swallowing the resulting error — a real
            // failure should still be reported.
            add_column_if_missing(conn, "mcp_servers", "slug", "TEXT")?;
            add_column_if_missing(conn, "mcp_servers", "tool_count", "INTEGER")?;
            add_column_if_missing(conn, "mcp_servers", "auth_ref", "TEXT")?;
            add_column_if_missing(conn, "cloud_connectors", "enabled", "INTEGER NOT NULL DEFAULT 1")?;
            add_column_if_missing(conn, "cloud_connectors", "models", "TEXT NOT NULL DEFAULT '[]'")?;

            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            Ok(())
        })
    }

    /// Run a closure with the connection held.
    ///
    /// Callers must not await inside `f`; all storage operations are synchronous
    /// by design.
    pub(crate) fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| KuroError::other("database lock poisoned"))?;
        f(&guard)
    }
}

/// Current time as an RFC 3339 string, the storage format for every timestamp.
pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Add a column only if the table does not already have it.
///
/// SQLite has no `ADD COLUMN IF NOT EXISTS`, and adding it unconditionally and
/// discarding the error would also discard a genuine failure. Asking the table
/// first keeps the migration honest.
fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    declaration: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let existing: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if existing.iter().any(|name| name == column) {
        return Ok(());
    }

    conn.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {declaration}"
    ))?;
    Ok(())
}

/// Decode a JSON column, falling back to a default rather than failing a read.
///
/// A row written by a newer version, or hand-edited, should not make an entire
/// list endpoint return an error.
pub(crate) fn json_or<T: serde::de::DeserializeOwned + Default>(raw: Option<&str>) -> T {
    raw.and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_a_fresh_database_once() {
        let db = Db::open_in_memory().expect("open");
        let version: i32 = db
            .with(|conn| Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);

        // Re-running migration must be a no-op rather than an error.
        db.migrate().expect("idempotent migrate");
    }

    #[test]
    fn enforces_foreign_keys() {
        let db = Db::open_in_memory().expect("open");
        let result = db.with(|conn| {
            conn.execute(
                "INSERT INTO messages (id, conversation_id, role, content, created_at)
                 VALUES ('m1', 'missing-conversation', 'user', 'hi', '2026-01-01T00:00:00Z')",
                [],
            )?;
            Ok(())
        });
        assert!(
            result.is_err(),
            "a message referencing a missing conversation must be rejected"
        );
    }
}
