//! Storage for coding workspaces and the changes made inside them.
//!
//! A workspace is a folder plus a mode. It is deliberately a different thing
//! from a project: a project groups conversations under standing instructions
//! and touches nothing on disk, while a workspace is the only place in Kuro
//! where a model can read or change a file.
//!
//! Every change is recorded with the file's previous contents. That is what
//! makes agent mode reasonable to offer at all — a change that can be put back
//! is a change the user can afford to let happen without being asked first.

use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use super::{now, Db};
use crate::workspace::WorkspaceMode;
use crate::{KuroError, Result};

/// Snapshots larger than this are recorded without their contents.
///
/// The log exists to undo edits to source files. A multi-megabyte file being
/// rewritten is not that, and keeping both sides of it would put the database
/// on a path to being larger than the project it describes. Such a change is
/// still listed; it just cannot be undone from here.
const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceRecord {
    pub id: String,
    pub name: String,
    /// The folder, as the user chose it.
    pub root_path: String,
    /// Default model for chats in this workspace. `None` inherits.
    pub model_id: Option<String>,
    /// Last mode used, so reopening a workspace resumes where it was left.
    pub mode: String,
    pub created_at: String,
    pub updated_at: String,
    /// Whether the folder is still there. Computed on read rather than stored,
    /// because it can change without Kuro being involved.
    pub root_exists: bool,
    pub conversation_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceChange {
    pub id: String,
    pub workspace_id: String,
    pub conversation_id: Option<String>,
    /// Path relative to the workspace root.
    pub path: String,
    /// `edit` or `write`.
    pub kind: String,
    /// Contents before the change. `None` means the file did not exist, or the
    /// snapshot was too large to keep.
    pub before: Option<String>,
    pub after: Option<String>,
    /// Whether this change has already been put back.
    pub undone: bool,
    pub created_at: String,
}

impl WorkspaceChange {
    /// Whether this change can still be reversed.
    pub fn is_undoable(&self) -> bool {
        !self.undone && self.after.is_some()
    }

    /// What undoing this change would do, given what is on disk right now.
    ///
    /// Separated from the route because it is the branch that decides whether
    /// to overwrite one of the user's files, and that decision should be
    /// readable and tested rather than buried in a handler.
    ///
    /// The comparison against `current` is the property that makes undo safe to
    /// offer at all: it can only ever remove a change the model made, never one
    /// the user made afterwards.
    pub fn plan_undo(&self, current: &str) -> UndoPlan {
        if self.undone {
            return UndoPlan::Refused("that change has already been undone".to_string());
        }
        let Some(after) = self.after.as_deref() else {
            return UndoPlan::Refused(
                "that file was too large to snapshot, so this change cannot be undone here"
                    .to_string(),
            );
        };
        if current != after {
            return UndoPlan::Refused(format!(
                "`{}` has changed since then, so undoing would discard that work.",
                self.path
            ));
        }

        match self.before.as_deref() {
            Some(before) => UndoPlan::Restore(before.to_string()),
            // The model created it, so undoing means removing it — safe,
            // because the contents were just confirmed to be exactly what the
            // model wrote and nothing else.
            None => UndoPlan::Remove,
        }
    }
}

/// What an undo would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoPlan {
    /// Put these contents back.
    Restore(String),
    /// Delete the file the model created.
    Remove,
    Refused(String),
}

fn read_workspace(row: &Row<'_>) -> rusqlite::Result<WorkspaceRecord> {
    let root_path: String = row.get("root_path")?;
    Ok(WorkspaceRecord {
        id: row.get("id")?,
        name: row.get("name")?,
        root_exists: std::path::Path::new(&root_path).is_dir(),
        root_path,
        model_id: row.get("model_id")?,
        mode: row.get("mode")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        conversation_count: row.get("conversation_count").unwrap_or(0),
    })
}

fn read_change(row: &Row<'_>) -> rusqlite::Result<WorkspaceChange> {
    Ok(WorkspaceChange {
        id: row.get("id")?,
        workspace_id: row.get("workspace_id")?,
        conversation_id: row.get("conversation_id")?,
        path: row.get("path")?,
        kind: row.get("kind")?,
        before: row.get("before_content")?,
        after: row.get("after_content")?,
        undone: row.get::<_, i64>("undone")? != 0,
        created_at: row.get("created_at")?,
    })
}

const WORKSPACE_COLUMNS: &str = "w.id, w.name, w.root_path, w.model_id, w.mode,
     w.created_at, w.updated_at";
const CONVERSATION_COUNT: &str = "(SELECT COUNT(*) FROM conversations c
      WHERE c.workspace_id = w.id) AS conversation_count";

impl Db {
    pub fn create_workspace(&self, name: &str, root_path: &str) -> Result<WorkspaceRecord> {
        let name = name.trim();
        if name.is_empty() {
            return Err(KuroError::bad_request("a workspace needs a name"));
        }
        let root = root_path.trim();
        if root.is_empty() {
            return Err(KuroError::bad_request("a workspace needs a folder"));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();

        self.with(|conn| {
            conn.execute(
                "INSERT INTO workspaces (id, name, root_path, mode, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, name, root, WorkspaceMode::default().as_str(), timestamp],
            )?;
            Ok(())
        })?;

        self.get_workspace(&id)?
            .ok_or_else(|| KuroError::other("workspace vanished after insert"))
    }

    pub fn get_workspace(&self, id: &str) -> Result<Option<WorkspaceRecord>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    &format!(
                        "SELECT {WORKSPACE_COLUMNS}, {CONVERSATION_COUNT}
                           FROM workspaces w WHERE w.id = ?1"
                    ),
                    params![id],
                    read_workspace,
                )
                .optional()?;
            Ok(found)
        })
    }

    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceRecord>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {WORKSPACE_COLUMNS}, {CONVERSATION_COUNT}
                   FROM workspaces w ORDER BY w.updated_at DESC"
            ))?;
            let rows = stmt
                .query_map([], read_workspace)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Update the parts of a workspace the interface can change.
    ///
    /// The root is not among them. A workspace that could be repointed would
    /// leave its recorded changes describing paths in a folder they were never
    /// made in, and the undo button would then write them somewhere new.
    pub fn update_workspace(
        &self,
        id: &str,
        name: Option<&str>,
        mode: Option<WorkspaceMode>,
        model_id: Option<Option<&str>>,
    ) -> Result<WorkspaceRecord> {
        self.with(|conn| {
            if let Some(name) = name {
                let name = name.trim();
                if name.is_empty() {
                    return Err(KuroError::bad_request("a workspace needs a name"));
                }
                conn.execute(
                    "UPDATE workspaces SET name = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, name, now()],
                )?;
            }
            if let Some(mode) = mode {
                conn.execute(
                    "UPDATE workspaces SET mode = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, mode.as_str(), now()],
                )?;
            }
            if let Some(model_id) = model_id {
                conn.execute(
                    "UPDATE workspaces SET model_id = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, model_id, now()],
                )?;
            }
            Ok(())
        })?;

        self.get_workspace(id)?
            .ok_or_else(|| KuroError::not_found(format!("workspace `{id}`")))
    }

    /// Delete a workspace. Its conversations are released, not deleted, and
    /// nothing on disk is touched — the folder is the user's work, and a record
    /// about it should never be able to destroy it.
    pub fn delete_workspace(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM workspaces WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn set_conversation_workspace(
        &self,
        conversation_id: &str,
        workspace_id: Option<&str>,
    ) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE conversations SET workspace_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![conversation_id, workspace_id, now()],
            )?;
            Ok(())
        })
    }

    pub fn list_workspace_conversations(&self, workspace_id: &str) -> Result<Vec<super::Conversation>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM conversations
                  WHERE workspace_id = ?1 AND archived = 0
                  ORDER BY updated_at DESC",
            )?;
            let rows = stmt
                .query_map(params![workspace_id], super::conversations::conversation_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Record a file change so it can be undone.
    pub fn record_workspace_change(
        &self,
        workspace_id: &str,
        conversation_id: Option<&str>,
        path: &str,
        kind: &str,
        before: Option<&str>,
        after: &str,
    ) -> Result<()> {
        // A snapshot that is too big to keep is stored as absent rather than
        // truncated. Half a file is worse than none: restoring it would delete
        // the rest while looking like it worked.
        let before = before.filter(|text| text.len() <= MAX_SNAPSHOT_BYTES);
        let after = (after.len() <= MAX_SNAPSHOT_BYTES).then_some(after);

        self.with(|conn| {
            conn.execute(
                "INSERT INTO workspace_changes (
                     id, workspace_id, conversation_id, path, kind,
                     before_content, after_content, undone, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    workspace_id,
                    conversation_id,
                    path,
                    kind,
                    before,
                    after,
                    now(),
                ],
            )?;
            conn.execute(
                "UPDATE workspaces SET updated_at = ?2 WHERE id = ?1",
                params![workspace_id, now()],
            )?;
            Ok(())
        })
    }

    pub fn list_workspace_changes(
        &self,
        workspace_id: &str,
        limit: usize,
    ) -> Result<Vec<WorkspaceChange>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM workspace_changes
                  WHERE workspace_id = ?1
                  ORDER BY created_at DESC, rowid DESC
                  LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(params![workspace_id, limit as i64], read_change)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_workspace_change(&self, id: &str) -> Result<Option<WorkspaceChange>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    "SELECT * FROM workspace_changes WHERE id = ?1",
                    params![id],
                    read_change,
                )
                .optional()?;
            Ok(found)
        })
    }

    pub fn mark_change_undone(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE workspace_changes SET undone = 1 WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workspace_round_trips_and_reports_a_missing_folder() {
        let db = Db::open_in_memory().expect("open");
        let root = std::env::temp_dir().join(format!("kuro-db-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("mkdir");

        let created = db
            .create_workspace("Kuro", &root.to_string_lossy())
            .expect("create");
        assert_eq!(created.mode, "plan", "planning is the safe default");
        assert!(created.root_exists);

        std::fs::remove_dir_all(&root).ok();
        let reloaded = db.get_workspace(&created.id).expect("get").expect("some");
        assert!(
            !reloaded.root_exists,
            "a folder that was moved must be visible as missing, not silently broken"
        );
    }

    #[test]
    fn a_workspace_needs_a_name_and_a_folder() {
        let db = Db::open_in_memory().expect("open");
        assert!(db.create_workspace("  ", "/tmp").is_err());
        assert!(db.create_workspace("Kuro", "   ").is_err());
    }

    #[test]
    fn the_mode_is_remembered_between_sessions() {
        let db = Db::open_in_memory().expect("open");
        let created = db.create_workspace("Kuro", "/tmp").expect("create");

        db.update_workspace(&created.id, None, Some(WorkspaceMode::Agent), None)
            .expect("update");

        assert_eq!(db.get_workspace(&created.id).unwrap().unwrap().mode, "agent");
    }

    #[test]
    fn deleting_a_workspace_releases_its_conversations_rather_than_destroying_them() {
        let db = Db::open_in_memory().expect("open");
        let workspace = db.create_workspace("Kuro", "/tmp").expect("create");
        let chat = db.create_conversation(None).expect("conversation");
        db.set_conversation_workspace(&chat.id, Some(&workspace.id))
            .expect("attach");
        assert_eq!(db.list_workspace_conversations(&workspace.id).unwrap().len(), 1);

        db.delete_workspace(&workspace.id).expect("delete");

        assert!(
            db.get_conversation(&chat.id).unwrap().is_some(),
            "the chats are the work; the workspace is a folder around them"
        );
    }

    #[test]
    fn changes_are_listed_newest_first_and_carry_both_sides() {
        let db = Db::open_in_memory().expect("open");
        let workspace = db.create_workspace("Kuro", "/tmp").expect("create");

        db.record_workspace_change(&workspace.id, None, "a.rs", "write", None, "new")
            .expect("record");
        db.record_workspace_change(&workspace.id, None, "b.rs", "edit", Some("old"), "changed")
            .expect("record");

        let changes = db.list_workspace_changes(&workspace.id, 10).expect("list");

        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].path, "b.rs");
        assert_eq!(changes[0].before.as_deref(), Some("old"));
        assert!(changes[0].is_undoable());
        assert!(changes[1].before.is_none(), "a created file has nothing before it");
    }

    #[test]
    fn an_oversized_snapshot_is_dropped_rather_than_truncated() {
        // Restoring half a file would delete the rest while looking like it
        // worked, so a change too big to snapshot is recorded as not undoable.
        let db = Db::open_in_memory().expect("open");
        let workspace = db.create_workspace("Kuro", "/tmp").expect("create");
        let huge = "x".repeat(MAX_SNAPSHOT_BYTES + 1);

        db.record_workspace_change(&workspace.id, None, "big.bin", "write", Some(&huge), &huge)
            .expect("record");

        let changes = db.list_workspace_changes(&workspace.id, 10).expect("list");
        assert!(changes[0].before.is_none());
        assert!(changes[0].after.is_none());
        assert!(!changes[0].is_undoable());
    }

    /// A change record standing alone, for the undo-decision tests.
    fn change(before: Option<&str>, after: Option<&str>, undone: bool) -> WorkspaceChange {
        WorkspaceChange {
            id: "c1".to_string(),
            workspace_id: "w1".to_string(),
            conversation_id: None,
            path: "src/main.rs".to_string(),
            kind: "edit".to_string(),
            before: before.map(str::to_string),
            after: after.map(str::to_string),
            undone,
            created_at: now(),
        }
    }

    #[test]
    fn undoing_an_edit_puts_the_previous_contents_back() {
        let edit = change(Some("old"), Some("new"), false);

        assert_eq!(edit.plan_undo("new"), UndoPlan::Restore("old".to_string()));
    }

    #[test]
    fn undoing_a_created_file_removes_it() {
        let creation = change(None, Some("fresh"), false);

        assert_eq!(creation.plan_undo("fresh"), UndoPlan::Remove);
    }

    #[test]
    fn undo_refuses_when_the_user_has_edited_the_file_since() {
        // The property the whole feature rests on. Undo may only ever remove a
        // change the model made; the moment the file differs from what the
        // model left, putting the old contents back would delete the user's own
        // work, which is far worse than refusing.
        let edit = change(Some("old"), Some("new"), false);

        let plan = edit.plan_undo("new, and then the user typed this");

        match plan {
            UndoPlan::Refused(reason) => assert!(reason.contains("has changed since")),
            other => panic!("must refuse, got {other:?}"),
        }
        // And the same guard applies to removing a file the model created.
        assert!(matches!(
            change(None, Some("fresh"), false).plan_undo("fresh + the user's line"),
            UndoPlan::Refused(_)
        ));
    }

    #[test]
    fn a_change_cannot_be_undone_twice() {
        assert!(matches!(
            change(Some("old"), Some("new"), true).plan_undo("new"),
            UndoPlan::Refused(_)
        ));
    }

    #[test]
    fn a_change_with_no_snapshot_cannot_be_undone() {
        assert!(matches!(
            change(Some("old"), None, false).plan_undo("whatever"),
            UndoPlan::Refused(_)
        ));
    }

    #[test]
    fn an_undone_change_is_not_offered_again() {
        let db = Db::open_in_memory().expect("open");
        let workspace = db.create_workspace("Kuro", "/tmp").expect("create");
        db.record_workspace_change(&workspace.id, None, "a.rs", "edit", Some("old"), "new")
            .expect("record");
        let change = db.list_workspace_changes(&workspace.id, 1).unwrap().remove(0);

        db.mark_change_undone(&change.id).expect("undo");

        let reloaded = db.get_workspace_change(&change.id).unwrap().unwrap();
        assert!(reloaded.undone);
        assert!(!reloaded.is_undoable());
    }
}
