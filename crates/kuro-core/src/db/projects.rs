//! Storage for projects.
//!
//! A project is standing instructions plus a grouping of conversations. The
//! instructions are the substance — saying "this is a Rust codebase, assume the
//! 2021 edition and tokio" once rather than at the top of every chat — and the
//! grouping is what makes them discoverable again a week later.
//!
//! Deleting a project releases its conversations rather than deleting them. The
//! chats are the work; the project is only a folder around them, and a folder
//! should not be able to destroy its contents by accident.

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{json_or, now, Db};
use crate::{KuroError, Result};

/// Instructions longer than this stop being guidance and start being a document
/// that crowds the conversation out of the context window.
const MAX_INSTRUCTION_CHARS: usize = 4000;

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Appended to the model's brief for every conversation in the project.
    pub instructions: String,
    /// Default model for new chats here. `None` inherits the global choice.
    pub model_id: Option<String>,
    /// Default tool groups for new chats here. `None` inherits the default.
    pub tool_groups: Option<Vec<String>>,
    pub created_at: String,
    pub updated_at: String,
    /// How many conversations live here, for the list page.
    pub conversation_count: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewProject {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default, rename = "modelId")]
    pub model_id: Option<String>,
    #[serde(default, rename = "toolGroups")]
    pub tool_groups: Option<Vec<String>>,
}

/// A partial update. Every field absent means "leave it alone", which is what
/// lets the instructions editor save without also having to send the model choice.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default, rename = "modelId")]
    pub model_id: Option<Option<String>>,
    #[serde(default, rename = "toolGroups")]
    pub tool_groups: Option<Option<Vec<String>>>,
}

impl Db {
    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS}, {COUNT_SUBQUERY}
                   FROM projects p ORDER BY p.updated_at DESC"
            ))?;
            let rows = stmt
                .query_map([], read_project)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    &format!(
                        "SELECT {COLUMNS}, {COUNT_SUBQUERY} FROM projects p WHERE p.id = ?1"
                    ),
                    params![id],
                    read_project,
                )
                .optional()?)
        })
    }

    pub fn insert_project(&self, input: &NewProject) -> Result<ProjectRecord> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(KuroError::bad_request("the project needs a name"));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        let instructions = truncate(input.instructions.as_deref().unwrap_or_default());
        let tool_groups = match &input.tool_groups {
            Some(groups) => Some(serde_json::to_string(groups)?),
            None => None,
        };

        self.with(|conn| {
            conn.execute(
                "INSERT INTO projects
                     (id, name, description, instructions, model_id, tool_groups,
                      created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    id,
                    name,
                    input.description.as_deref().unwrap_or_default().trim(),
                    instructions,
                    input.model_id.as_deref(),
                    tool_groups,
                    timestamp,
                ],
            )?;
            Ok(())
        })?;

        self.get_project(&id)?
            .ok_or_else(|| KuroError::other("the project disappeared immediately after insert"))
    }

    pub fn update_project(&self, id: &str, patch: &ProjectUpdate) -> Result<ProjectRecord> {
        let existing = self
            .get_project(id)?
            .ok_or_else(|| KuroError::not_found(format!("project `{id}`")))?;

        let name = match &patch.name {
            Some(name) if name.trim().is_empty() => {
                return Err(KuroError::bad_request("the project needs a name"))
            }
            Some(name) => name.trim().to_string(),
            None => existing.name,
        };
        let description = patch
            .description
            .as_deref()
            .map(|text| text.trim().to_string())
            .unwrap_or(existing.description);
        let instructions = patch
            .instructions
            .as_deref()
            .map(truncate)
            .unwrap_or(existing.instructions);

        // Double option: absent leaves it, `Some(None)` clears it.
        let model_id = match &patch.model_id {
            Some(value) => value.clone(),
            None => existing.model_id,
        };
        let tool_groups = match &patch.tool_groups {
            Some(Some(groups)) => Some(serde_json::to_string(groups)?),
            Some(None) => None,
            None => match &existing.tool_groups {
                Some(groups) => Some(serde_json::to_string(groups)?),
                None => None,
            },
        };

        self.with(|conn| {
            conn.execute(
                "UPDATE projects
                    SET name = ?2, description = ?3, instructions = ?4,
                        model_id = ?5, tool_groups = ?6, updated_at = ?7
                  WHERE id = ?1",
                params![id, name, description, instructions, model_id, tool_groups, now()],
            )?;
            Ok(())
        })?;

        self.get_project(id)?
            .ok_or_else(|| KuroError::other("the project disappeared after update"))
    }

    /// Delete a project. Its conversations survive, released to the top level.
    pub fn delete_project(&self, id: &str) -> Result<bool> {
        self.with(|conn| {
            let removed = conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
            Ok(removed > 0)
        })
    }

    /// Move a conversation into a project, or out of one with `None`.
    pub fn set_conversation_project(
        &self,
        conversation_id: &str,
        project_id: Option<&str>,
    ) -> Result<()> {
        if let Some(project) = project_id {
            if self.get_project(project)?.is_none() {
                return Err(KuroError::not_found(format!("project `{project}`")));
            }
        }

        self.with(|conn| {
            let changed = conn.execute(
                "UPDATE conversations SET project_id = ?2 WHERE id = ?1",
                params![conversation_id, project_id],
            )?;
            if changed == 0 {
                return Err(KuroError::not_found(format!(
                    "conversation `{conversation_id}`"
                )));
            }
            Ok(())
        })
    }

    /// Conversations in a project, newest first.
    ///
    /// One indexed query rather than asking each conversation which project it is
    /// in, which would be an N+1 on the page that exists to list them.
    pub fn list_project_conversations(&self, project_id: &str) -> Result<Vec<super::Conversation>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, title_mode, model_id, pinned, archived, created_at,
                        updated_at, forked_from_id, workspace_id
                   FROM conversations
                  WHERE project_id = ?1 AND archived = 0
                  ORDER BY updated_at DESC",
            )?;
            let rows = stmt
                .query_map(params![project_id], |row| {
                    Ok(super::Conversation {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        title_mode: row.get(2)?,
                        model_id: row.get(3)?,
                        pinned: row.get::<_, i64>(4)? != 0,
                        archived: row.get::<_, i64>(5)? != 0,
                        created_at: row.get(6)?,
                        updated_at: row.get(7)?,
                        forked_from_id: row.get(8)?,
                        workspace_id: row.get(9)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// The project a conversation belongs to, if any.
    pub fn project_for_conversation(&self, conversation_id: &str) -> Result<Option<ProjectRecord>> {
        let id: Option<String> = self.with(|conn| {
            Ok(conn
                .query_row(
                    "SELECT project_id FROM conversations WHERE id = ?1",
                    params![conversation_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten())
        })?;

        match id {
            Some(project_id) => self.get_project(&project_id),
            None => Ok(None),
        }
    }
}

const COLUMNS: &str = "p.id, p.name, p.description, p.instructions, p.model_id, \
                       p.tool_groups, p.created_at, p.updated_at";

/// Counted in the same statement rather than with a query per row, which would be
/// an N+1 on a page that exists to list projects.
const COUNT_SUBQUERY: &str =
    "(SELECT COUNT(*) FROM conversations c WHERE c.project_id = p.id AND c.archived = 0)";

fn read_project(row: &Row<'_>) -> rusqlite::Result<ProjectRecord> {
    let tool_groups: Option<String> = row.get(5)?;

    Ok(ProjectRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        instructions: row.get(3)?,
        model_id: row.get(4)?,
        tool_groups: tool_groups.as_deref().map(|raw| json_or(Some(raw))),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        conversation_count: row.get(8)?,
    })
}

fn truncate(text: &str) -> String {
    text.trim().chars().take(MAX_INSTRUCTION_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(db: &Db, name: &str) -> ProjectRecord {
        db.insert_project(&NewProject {
            name: name.to_string(),
            instructions: Some("Assume Rust 2021 and tokio.".to_string()),
            ..Default::default()
        })
        .expect("insert")
    }

    fn conversation(db: &Db) -> String {
        db.create_conversation(None).expect("conversation").id
    }

    #[test]
    fn inserts_and_reads_back_a_project() {
        let db = Db::open_in_memory().expect("open");
        let created = project(&db, "Kuro");

        assert_eq!(created.name, "Kuro");
        assert!(created.instructions.contains("tokio"));
        assert_eq!(created.conversation_count, 0);
        assert_eq!(created.model_id, None);
        assert_eq!(created.tool_groups, None);

        let listed = db.list_projects().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);
    }

    #[test]
    fn rejects_a_project_with_no_name() {
        let db = Db::open_in_memory().expect("open");
        assert!(db
            .insert_project(&NewProject {
                name: "   ".to_string(),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn stores_model_and_tool_defaults() {
        let db = Db::open_in_memory().expect("open");
        let created = db
            .insert_project(&NewProject {
                name: "Research".to_string(),
                model_id: Some("qwen3-4b:q4_k_m".to_string()),
                tool_groups: Some(vec!["web".to_string(), "memory".to_string()]),
                ..Default::default()
            })
            .expect("insert");

        assert_eq!(created.model_id.as_deref(), Some("qwen3-4b:q4_k_m"));
        assert_eq!(
            created.tool_groups,
            Some(vec!["web".to_string(), "memory".to_string()])
        );
    }

    #[test]
    fn a_partial_update_leaves_untouched_fields_alone() {
        let db = Db::open_in_memory().expect("open");
        let created = db
            .insert_project(&NewProject {
                name: "Kuro".to_string(),
                description: Some("The server".to_string()),
                instructions: Some("Be terse.".to_string()),
                model_id: Some("m1".to_string()),
                ..Default::default()
            })
            .expect("insert");

        let updated = db
            .update_project(
                &created.id,
                &ProjectUpdate {
                    instructions: Some("Be terse and cite sources.".to_string()),
                    ..Default::default()
                },
            )
            .expect("update");

        assert_eq!(updated.instructions, "Be terse and cite sources.");
        assert_eq!(updated.name, "Kuro", "name was not sent and must not change");
        assert_eq!(updated.description, "The server");
        assert_eq!(updated.model_id.as_deref(), Some("m1"));
    }

    #[test]
    fn a_model_choice_can_be_cleared_as_distinct_from_left_alone() {
        let db = Db::open_in_memory().expect("open");
        let created = db
            .insert_project(&NewProject {
                name: "Kuro".to_string(),
                model_id: Some("m1".to_string()),
                ..Default::default()
            })
            .expect("insert");

        let cleared = db
            .update_project(
                &created.id,
                &ProjectUpdate {
                    model_id: Some(None),
                    ..Default::default()
                },
            )
            .expect("update");

        assert_eq!(cleared.model_id, None, "Some(None) must clear, not be ignored");
    }

    #[test]
    fn an_update_cannot_blank_the_name() {
        let db = Db::open_in_memory().expect("open");
        let created = project(&db, "Kuro");

        assert!(db
            .update_project(
                &created.id,
                &ProjectUpdate {
                    name: Some("  ".to_string()),
                    ..Default::default()
                }
            )
            .is_err());
    }

    #[test]
    fn updating_a_missing_project_is_a_not_found() {
        let db = Db::open_in_memory().expect("open");
        let error = db.update_project("nope", &ProjectUpdate::default()).unwrap_err();
        assert!(matches!(error, KuroError::NotFound(_)));
    }

    #[test]
    fn oversized_instructions_are_truncated_not_rejected() {
        let db = Db::open_in_memory().expect("open");
        let created = db
            .insert_project(&NewProject {
                name: "Big".to_string(),
                instructions: Some("x".repeat(MAX_INSTRUCTION_CHARS + 900)),
                ..Default::default()
            })
            .expect("insert");

        assert_eq!(created.instructions.chars().count(), MAX_INSTRUCTION_CHARS);
    }

    #[test]
    fn conversations_move_in_and_out_of_a_project() {
        let db = Db::open_in_memory().expect("open");
        let created = project(&db, "Kuro");
        let chat = conversation(&db);

        db.set_conversation_project(&chat, Some(&created.id)).expect("move in");

        assert_eq!(
            db.project_for_conversation(&chat)
                .expect("lookup")
                .map(|p| p.id),
            Some(created.id.clone())
        );
        assert_eq!(
            db.get_project(&created.id).expect("get").expect("present").conversation_count,
            1
        );

        db.set_conversation_project(&chat, None).expect("move out");
        assert!(db.project_for_conversation(&chat).expect("lookup").is_none());
    }

    #[test]
    fn moving_into_a_missing_project_is_refused() {
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);
        assert!(db.set_conversation_project(&chat, Some("nope")).is_err());
    }

    #[test]
    fn moving_a_missing_conversation_is_refused() {
        let db = Db::open_in_memory().expect("open");
        let created = project(&db, "Kuro");
        assert!(db.set_conversation_project("nope", Some(&created.id)).is_err());
    }

    #[test]
    fn deleting_a_project_releases_its_conversations_rather_than_destroying_them() {
        let db = Db::open_in_memory().expect("open");
        let created = project(&db, "Kuro");
        let chat = conversation(&db);
        db.set_conversation_project(&chat, Some(&created.id)).expect("move in");

        assert!(db.delete_project(&created.id).expect("delete"));

        assert!(
            db.get_conversation(&chat).expect("get").is_some(),
            "the chats are the work; deleting a folder must not delete them"
        );
        assert!(db.project_for_conversation(&chat).expect("lookup").is_none());
        assert!(!db.delete_project(&created.id).expect("second delete"));
    }

    #[test]
    fn a_conversation_in_no_project_has_none() {
        let db = Db::open_in_memory().expect("open");
        let chat = conversation(&db);
        assert!(db.project_for_conversation(&chat).expect("lookup").is_none());
    }

    #[test]
    fn looking_up_a_missing_conversation_yields_none_rather_than_an_error() {
        let db = Db::open_in_memory().expect("open");
        assert!(db.project_for_conversation("nope").expect("lookup").is_none());
    }
}
