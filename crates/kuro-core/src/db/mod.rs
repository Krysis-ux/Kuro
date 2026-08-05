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
mod projects;
mod runtimes;
mod skills;
mod usage;
mod workspaces;

pub use cloud::{CloudConnectorRecord, CloudStatus, NewCloudConnector};
pub use conversations::{Conversation, Message, MessageCompletion, NewMessage};
pub use downloads::{DownloadKind, DownloadRecord, DownloadStatus};
pub use mcp::{McpServerRecord, McpStatus, McpTransport, NewMcpServer};
pub use memories::MemoryRecord;
pub use models::{ModelRecord, ModelSource, ModelStatus, NewModel};
pub use projects::{NewProject, ProjectRecord, ProjectUpdate};
pub use runtimes::EngineRuntimeRecord;
pub use skills::{UserSkillRecord, MAX_INSTRUCTION_CHARS as MAX_USER_SKILL_CHARS};
pub use usage::ProviderUsage;
pub use workspaces::{UndoPlan, WorkspaceChange, WorkspaceRecord};

const SCHEMA: &str = include_str!("schema.sql");

/// Indexes over columns that `add_column_if_missing` may have only just created.
/// Run after that step; see the ordering note in [`Db::migrate`].
const LATE_INDEXES: &str = "
    CREATE INDEX IF NOT EXISTS idx_conversations_project
        ON conversations (project_id, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_conversations_workspace
        ON conversations (workspace_id, updated_at DESC);
    -- Partial, so it covers only the turns that went to a provider. A machine
    -- full of local chats pays almost nothing for it.
    CREATE INDEX IF NOT EXISTS idx_messages_usage
        ON messages (provider_slug, created_at)
        WHERE provider_slug IS NOT NULL;
";
// Bumped to 7 when `user_skills` arrived. The schema file is only replayed when
// this number moves, so adding a `CREATE TABLE IF NOT EXISTS` without touching
// it produces a table that exists on a fresh install and nowhere else — which
// is a bug that passes every test, because tests open an empty database.
const SCHEMA_VERSION: i32 = 7;

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

            // Three ordered phases. The order is load-bearing.
            //
            // 1. Tables and indexes that depend only on themselves. Every
            //    statement is `IF NOT EXISTS`, so replaying the whole file brings
            //    an older database up to date.
            conn.execute_batch(SCHEMA)?;

            // 2. Columns added to a table that already shipped. `IF NOT EXISTS` is
            //    not available for `ALTER TABLE`, so each one is checked first
            //    rather than adding it and swallowing the resulting error — a real
            //    failure should still be reported.
            add_column_if_missing(conn, "mcp_servers", "slug", "TEXT")?;
            add_column_if_missing(conn, "mcp_servers", "tool_count", "INTEGER")?;
            add_column_if_missing(conn, "mcp_servers", "auth_ref", "TEXT")?;
            add_column_if_missing(conn, "cloud_connectors", "enabled", "INTEGER NOT NULL DEFAULT 1")?;
            add_column_if_missing(conn, "cloud_connectors", "models", "TEXT NOT NULL DEFAULT '[]'")?;
            add_column_if_missing(conn, "conversations", "project_id", "TEXT")?;
            add_column_if_missing(conn, "conversations", "forked_from_id", "TEXT")?;
            add_column_if_missing(conn, "conversations", "workspace_id", "TEXT")?;
            // Which provider's allowance a turn spent. Distinct from
            // `model_id`, which on a pooled turn records the pool.
            add_column_if_missing(conn, "messages", "provider_slug", "TEXT")?;
            // Prompt tokens summed across every tool round, rather than the
            // last round only. See `Aggregate::absorb`.
            add_column_if_missing(conn, "messages", "usage_prompt_tokens_total", "INTEGER")?;

            // 3. Indexes over columns phase 2 may have just added. These cannot
            //    live in the schema file: on a fresh database the column is part
            //    of `CREATE TABLE` and the index would work there, but on an
            //    upgrade the column does not exist when the file runs, the batch
            //    fails, and the daemon refuses to start.
            conn.execute_batch(LATE_INDEXES)?;

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

    // An empty result means the table does not exist. That is not a failure: on a
    // fresh database the schema file creates the table with this column already
    // in it, so there is nothing to add.
    if existing.is_empty() || existing.iter().any(|name| name == column) {
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

    /// The shape version 1 shipped with: no `project_id`, no `memories`, and
    /// `mcp_servers`/`cloud_connectors` without the columns added since.
    ///
    /// Kept verbatim rather than generated, because the whole point is to migrate
    /// from what real users actually have on disk.
    const V1_SCHEMA: &str = "
        CREATE TABLE conversations (
            id           TEXT PRIMARY KEY,
            title        TEXT NOT NULL DEFAULT 'New chat',
            title_mode   TEXT NOT NULL DEFAULT 'first_line',
            model_id     TEXT,
            pinned       INTEGER NOT NULL DEFAULT 0,
            archived     INTEGER NOT NULL DEFAULT 0,
            created_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL
        );
        CREATE TABLE messages (
            id              TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
            role            TEXT NOT NULL,
            content         TEXT NOT NULL DEFAULT '',
            reasoning_content TEXT, tool_calls TEXT, tool_call_id TEXT, attachments TEXT,
            used_web_search INTEGER NOT NULL DEFAULT 0, web_sources TEXT, model_id TEXT,
            usage_prompt_tokens INTEGER, usage_completion_tokens INTEGER,
            timing_ttft_ms INTEGER, timing_total_ms INTEGER, timing_tokens_per_sec REAL,
            finish_reason TEXT, created_at TEXT NOT NULL
        );
        CREATE TABLE mcp_servers (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, transport TEXT NOT NULL,
            command TEXT, args TEXT, env TEXT, url TEXT, headers TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            status TEXT NOT NULL DEFAULT 'disconnected',
            last_error TEXT, created_at TEXT NOT NULL
        );
        CREATE TABLE cloud_connectors (
            id TEXT PRIMARY KEY, provider TEXT NOT NULL, label TEXT NOT NULL,
            keychain_ref TEXT NOT NULL, base_url TEXT,
            status TEXT NOT NULL DEFAULT 'untested',
            last_tested_at TEXT, last_error TEXT, created_at TEXT NOT NULL
        );
        CREATE TABLE settings (
            key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL
        );
    ";

    /// A version-1 database with a conversation and a message in it.
    fn version_one_database() -> Connection {
        let conn = Connection::open_in_memory().expect("open");
        conn.execute_batch(V1_SCHEMA).expect("v1 schema");
        conn.execute_batch(
            "INSERT INTO conversations (id, title, created_at, updated_at)
                 VALUES ('c1', 'What is a black hole?', '2026-01-01', '2026-01-01');
             INSERT INTO messages (id, conversation_id, role, content, created_at)
                 VALUES ('m1', 'c1', 'user', 'explain', '2026-01-01');",
        )
        .expect("seed");
        conn.pragma_update(None, "user_version", 1).expect("version");
        conn
    }

    #[test]
    fn upgrades_a_version_one_database_without_losing_anything() {
        // This is the path every existing install takes. It is separate from the
        // fresh-database test because the two fail in different ways: a fresh
        // database gets `project_id` from `CREATE TABLE`, so an index over it
        // succeeds there and fails only here.
        let db = Db::from_connection(version_one_database()).expect("migrate v1");

        let version: i32 = db
            .with(|conn| Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);

        let conversations = db.list_conversations(None).expect("conversations");
        assert_eq!(conversations.len(), 1, "existing chats must survive an upgrade");
        assert_eq!(conversations[0].title, "What is a black hole?");
        assert_eq!(db.list_messages("c1").expect("messages").len(), 1);
    }

    #[test]
    fn an_upgraded_database_has_everything_the_new_code_reads() {
        let db = Db::from_connection(version_one_database()).expect("migrate v1");

        // Tables added after version 1.
        assert!(db.list_projects().expect("projects").is_empty());
        assert_eq!(db.count_memories().expect("memories"), 0);

        // Columns added after version 1, exercised through the accessors that
        // read them rather than by inspecting the schema.
        assert!(db.list_mcp_servers().expect("mcp").is_empty());
        assert!(db.list_cloud_connectors().expect("providers").is_empty());

        // And the index that caused the original failure now exists.
        let indexes: Vec<String> = db
            .with(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'conversations'",
                )?;
                let names = stmt
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(names)
            })
            .expect("indexes");
        assert!(
            indexes.iter().any(|name| name == "idx_conversations_project"),
            "got: {indexes:?}"
        );
    }

    #[test]
    fn a_project_can_be_used_immediately_after_an_upgrade() {
        let db = Db::from_connection(version_one_database()).expect("migrate v1");

        let project = db
            .insert_project(&crate::db::NewProject {
                name: "Kuro".to_string(),
                ..Default::default()
            })
            .expect("insert");
        db.set_conversation_project("c1", Some(&project.id))
            .expect("move the pre-existing chat into a new project");

        assert_eq!(
            db.list_project_conversations(&project.id).expect("list").len(),
            1
        );
    }

    #[test]
    fn migrating_an_already_current_database_is_a_no_op() {
        let db = Db::open_in_memory().expect("open");
        db.create_conversation(None).expect("conversation");

        db.migrate().expect("second migrate");

        assert_eq!(db.list_conversations(None).expect("list").len(), 1);
    }

    #[test]
    fn adding_a_column_to_a_table_that_does_not_exist_is_not_an_error() {
        // Ordering inside `migrate` should not depend on which tables happen to
        // exist yet.
        let conn = Connection::open_in_memory().expect("open");
        add_column_if_missing(&conn, "not_a_table", "whatever", "TEXT").expect("no-op");
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
