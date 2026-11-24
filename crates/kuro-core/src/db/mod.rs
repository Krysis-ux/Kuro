//! SQLite storage: one connection behind a mutex (single-user local app). Queries
//! are small, so the lock is held briefly and never across an `.await`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::{KuroError, Result};

mod agents;
mod automations;
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

pub use agents::UserAgentRecord;
pub use automations::{AutomationInput, AutomationRecord, Schedule};
pub use cloud::{CloudConnectorRecord, CloudStatus, NewCloudConnector};
pub use conversations::{Conversation, Message, MessageCompletion, NewMessage};
pub use downloads::{DownloadKind, DownloadRecord, DownloadStatus};
pub use mcp::{McpServerRecord, McpStatus, McpTransport, NewMcpServer};
pub use memories::MemoryRecord;
pub use models::{ModelRecord, ModelSource, ModelStatus, NewModel};
pub use projects::{NewProject, ProjectRecord, ProjectUpdate};
pub use runtimes::EngineRuntimeRecord;
pub use skills::{UserSkillRecord, MAX_INSTRUCTION_CHARS as MAX_USER_SKILL_CHARS};
pub use usage::{ModelUsage, ProviderUsage};
pub use workspaces::{UndoPlan, WorkspaceChange, WorkspaceRecord};

const SCHEMA: &str = include_str!("schema.sql");

/// Indexes over columns that `add_column_if_missing` may have just created; run
/// after that step (see [`Db::migrate`]).
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
// Bump whenever schema.sql changes: the file is only replayed when this number
// moves, so a new table would otherwise exist only on fresh installs (and tests,
// which open an empty database, would not notice).
const SCHEMA_VERSION: i32 = 9;

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
        // WAL so reads don't block the writer (download progress during a chat).
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
            // Equal means nothing to do. Higher is not "up to date": another build of
            // Kuro sharing this data folder may have stamped a bigger number without
            // creating this build's tables, so replay on any difference. Every phase
            // below is idempotent, so a replay is cheap and safe.
            if current == SCHEMA_VERSION {
                return Ok(());
            }

            // Three ordered phases; the order is load-bearing.
            //
            // 1. Tables and indexes depending only on themselves. All are
            //    `IF NOT EXISTS`, so replaying the file upgrades an older database.
            conn.execute_batch(SCHEMA)?;

            // 2. Columns added to tables that already shipped. `ALTER TABLE` has no
            //    `IF NOT EXISTS`, so each is checked first (a real failure must
            //    still surface).
            add_column_if_missing(conn, "mcp_servers", "slug", "TEXT")?;
            add_column_if_missing(conn, "mcp_servers", "tool_count", "INTEGER")?;
            add_column_if_missing(conn, "mcp_servers", "auth_ref", "TEXT")?;
            add_column_if_missing(conn, "cloud_connectors", "enabled", "INTEGER NOT NULL DEFAULT 1")?;
            add_column_if_missing(conn, "cloud_connectors", "models", "TEXT NOT NULL DEFAULT '[]'")?;
            add_column_if_missing(conn, "conversations", "project_id", "TEXT")?;
            add_column_if_missing(conn, "conversations", "forked_from_id", "TEXT")?;
            add_column_if_missing(conn, "conversations", "workspace_id", "TEXT")?;
            // Provider whose allowance a turn spent; `model_id` records the pool.
            add_column_if_missing(conn, "messages", "provider_slug", "TEXT")?;
            // Prompt tokens summed across all tool rounds (see `Aggregate::absorb`).
            add_column_if_missing(conn, "messages", "usage_prompt_tokens_total", "INTEGER")?;

            // 3. Indexes over columns phase 2 may have just added. They cannot live
            //    in the schema file: on an upgrade the column does not exist yet
            //    when the file runs, the batch fails, and the daemon won't start.
            conn.execute_batch(LATE_INDEXES)?;

            // Never lower the stamp: the build that set a higher one must not be made
            // to think its own tables are missing.
            conn.pragma_update(None, "user_version", current.max(SCHEMA_VERSION))?;
            Ok(())
        })
    }

    /// Run a closure with the connection held. Must not await inside `f`.
    pub(crate) fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| KuroError::other("database lock poisoned"))?;
        f(&guard)
    }
}

/// Current time as RFC 3339, the storage format for all timestamps.
pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Add a column only if the table lacks it (SQLite has no `ADD COLUMN IF NOT
/// EXISTS`, and swallowing the error would hide real failures).
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

    // Empty means the table doesn't exist; the schema file creates it with the
    // column already in it.
    if existing.is_empty() || existing.iter().any(|name| name == column) {
        return Ok(());
    }

    conn.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {declaration}"
    ))?;
    Ok(())
}

/// Decode a JSON column, falling back to the default so one bad row (newer
/// version, hand edit) doesn't fail a whole list.
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

        // Re-running must be a no-op.
        db.migrate().expect("idempotent migrate");
    }

    #[test]
    fn a_database_stamped_higher_by_another_build_still_gets_this_builds_tables() {
        let db = Db::open_in_memory().expect("open");
        db.with(|conn| {
            conn.execute_batch("DROP TABLE user_agents;")?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION + 5)?;
            Ok(())
        })
        .expect("simulate a newer build's database");

        db.migrate().expect("migrate");

        let (tables, version): (i64, i32) = db
            .with(|conn| {
                let tables = conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'user_agents'",
                    [],
                    |row| row.get(0),
                )?;
                let version = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
                Ok((tables, version))
            })
            .expect("inspect");
        assert_eq!(tables, 1, "the missing table is created");
        assert_eq!(version, SCHEMA_VERSION + 5, "the higher stamp is kept");
    }

    /// Schema as shipped in version 1 (no `project_id`, no `memories`, older
    /// `mcp_servers`/`cloud_connectors`). Kept verbatim: it is what real users have.
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
        // Separate from the fresh-database test: a fresh database gets `project_id`
        // from `CREATE TABLE`, so an index over it only fails on upgrade.
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

        // Columns added after version 1, checked through the accessors that read them.
        assert!(db.list_mcp_servers().expect("mcp").is_empty());
        assert!(db.list_cloud_connectors().expect("providers").is_empty());

        // The index over the added column exists.
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
        // Migration order must not depend on which tables exist yet.
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
