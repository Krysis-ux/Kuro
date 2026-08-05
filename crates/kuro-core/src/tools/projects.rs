//! Letting a chat read the code, without letting it touch the code.
//!
//! Chat and the Code page were kept completely apart, and the separation cost
//! something real: asked "what does the auth module in my app do", a chat had to
//! say it could not see any files, while a workspace three clicks away could
//! read the whole thing. The user then pastes the file in by hand, which is the
//! same information reaching the same model by a worse route.
//!
//! So this is the narrow bridge. A chat can list the workspaces the user made,
//! read a file out of one, and search across one. That is all it can do, and the
//! ceiling is structural rather than a promise: these tools construct their
//! [`Workspace`] at [`WorkspaceMode::Plan`] regardless of what the workspace is
//! actually set to, so the permission object they hand to the containment layer
//! has no write tier to grant. A chat cannot reach `edit_file` because a chat is
//! never offered it, and cannot reach a write through these because there is no
//! code path from here to one.
//!
//! [`Workspace`]: crate::workspace::Workspace
//! [`WorkspaceMode::Plan`]: crate::workspace::WorkspaceMode::Plan

use std::path::PathBuf;

use serde_json::Value;

use crate::db::{Db, WorkspaceRecord};
use crate::tools::{files, ToolOutcome};
use crate::workspace::{search, Workspace, WorkspaceMode};

/// Workspaces listed at once. More than this and the answer is a directory
/// listing rather than an orientation.
const MAX_LISTED: usize = 40;

/// Every workspace the user has made, so a chat can say what exists.
pub fn list(db: &Db) -> ToolOutcome {
    let held = match db.list_workspaces() {
        Ok(held) => held,
        Err(error) => return ToolOutcome::failed(error),
    };

    if held.is_empty() {
        return ToolOutcome::ok(
            "There are no coding workspaces yet. The user makes one on the Code page by \
             choosing a folder on this computer.",
        );
    }

    let mut out = String::from(
        "The user's coding workspaces. You can read and search these, and you cannot change \
         them — changing files happens on the Code page.\n\n",
    );

    for workspace in held.iter().take(MAX_LISTED) {
        out.push_str(&format!(
            "- `{}` — {} ({}){}\n",
            workspace.name,
            workspace.root_path,
            workspace.mode,
            if workspace.root_exists {
                ""
            } else {
                " — the folder is no longer on disk"
            },
        ));
    }

    if held.len() > MAX_LISTED {
        out.push_str(&format!("\n…and {} more.\n", held.len() - MAX_LISTED));
    }

    out.push_str(
        "\nUse the name in `project` when calling read_project_file or search_projects.",
    );
    ToolOutcome::ok(out)
}

/// Read one file out of one workspace.
pub fn read_file(db: &Db, arguments: &Value) -> ToolOutcome {
    let Some(project) = string_argument(arguments, "project") else {
        return ToolOutcome::failed("`project` is required. Call list_projects to see the names.");
    };
    let Some(path) = string_argument(arguments, "path") else {
        return ToolOutcome::failed("`path` is required and must be a string");
    };

    let record = match resolve(db, &project) {
        Ok(record) => record,
        Err(outcome) => return outcome,
    };

    match readable(&record).permissions().resolve_path(&path, false) {
        Ok(resolved) => match files::read_file(&resolved) {
            Ok(text) => ToolOutcome::ok(format!("`{path}` in `{}`:\n\n{text}", record.name)),
            Err(error) => ToolOutcome::failed(error),
        },
        Err(error) => ToolOutcome::failed(error),
    }
}

/// Search one workspace, or list its layout when no query is given.
pub fn search_project(db: &Db, arguments: &Value) -> ToolOutcome {
    let Some(project) = string_argument(arguments, "project") else {
        return ToolOutcome::failed("`project` is required. Call list_projects to see the names.");
    };

    let record = match resolve(db, &project) {
        Ok(record) => record,
        Err(outcome) => return outcome,
    };
    let root = PathBuf::from(&record.root_path);

    // No query means "show me the shape of this", which is the first thing
    // anybody wants and would otherwise need a second tool.
    let Some(query) = string_argument(arguments, "query") else {
        return match search::tree(&root) {
            Ok(entries) => ToolOutcome::ok(format!(
                "The layout of `{}`:\n\n{}",
                record.name,
                search::format_tree(&root, &entries)
            )),
            Err(error) => ToolOutcome::failed(error),
        };
    };

    let case_sensitive = arguments
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    match search::find_text(&root, &query, case_sensitive) {
        Ok(found) => ToolOutcome::ok(format!(
            "In `{}`:\n\n{}",
            record.name,
            search::format_matches(&query, &found)
        )),
        Err(error) => ToolOutcome::failed(error),
    }
}

/// Find a workspace by name or id.
///
/// By name first, because that is what the model was shown and what the user
/// says out loud. An id still works, for a model that kept one from an earlier
/// turn.
fn resolve(db: &Db, needle: &str) -> Result<WorkspaceRecord, ToolOutcome> {
    let held = db.list_workspaces().map_err(ToolOutcome::failed)?;

    let found = held
        .iter()
        .find(|workspace| workspace.name.eq_ignore_ascii_case(needle))
        .or_else(|| held.iter().find(|workspace| workspace.id == needle))
        // A partial name is what a model produces when it paraphrases, and
        // refusing that costs a round trip to learn nothing.
        .or_else(|| {
            held.iter().find(|workspace| {
                workspace
                    .name
                    .to_ascii_lowercase()
                    .contains(&needle.to_ascii_lowercase())
            })
        });

    let Some(found) = found else {
        let names: Vec<&str> = held.iter().map(|held| held.name.as_str()).collect();
        return Err(ToolOutcome::failed(if names.is_empty() {
            "there are no coding workspaces yet".to_string()
        } else {
            format!(
                "there is no workspace called `{needle}`. There is: {}",
                names.join(", ")
            )
        }));
    };

    if !found.root_exists {
        return Err(ToolOutcome::failed(format!(
            "the folder for `{}` ({}) is no longer on disk",
            found.name, found.root_path
        )));
    }

    Ok(found.clone())
}

/// The enforcement object for a workspace, always read-only.
///
/// The stored mode is deliberately ignored. A workspace set to Agent is set that
/// way for the Code page; it is not consent for a chat to start writing, and
/// reading the stored value here would make it exactly that.
fn readable(record: &WorkspaceRecord) -> Workspace {
    Workspace {
        id: record.id.clone(),
        root: PathBuf::from(&record.root_path),
        mode: WorkspaceMode::Plan,
    }
}

fn string_argument(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fixture {
        db: Db,
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("kuro-proj-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(root.join("src")).expect("mkdir");
            std::fs::write(root.join("src/auth.rs"), "fn sign_in() { todo!() }\n").expect("write");

            let db = Db::open_in_memory().expect("open");
            db.create_workspace("My App", &root.to_string_lossy())
                .expect("workspace");

            Self { db, root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).ok();
        }
    }

    #[test]
    fn a_chat_with_no_workspaces_is_told_where_they_come_from() {
        let db = Db::open_in_memory().expect("open");
        let outcome = list(&db);

        assert!(!outcome.is_error);
        assert!(outcome.content.contains("Code page"));
    }

    #[test]
    fn workspaces_are_listed_with_their_folders() {
        let fixture = Fixture::new();
        let outcome = list(&fixture.db);

        assert!(outcome.content.contains("My App"));
        assert!(outcome.content.contains(&fixture.root.to_string_lossy().to_string()));
        assert!(
            outcome.content.contains("cannot change"),
            "the model must not offer to edit through a read-only tool"
        );
    }

    #[test]
    fn a_file_can_be_read_out_of_a_named_project() {
        let fixture = Fixture::new();

        let outcome = read_file(
            &fixture.db,
            &json!({ "project": "My App", "path": "src/auth.rs" }),
        );

        assert!(!outcome.is_error, "{}", outcome.content);
        assert!(outcome.content.contains("sign_in"));
    }

    #[test]
    fn a_project_can_be_named_loosely_or_by_id() {
        let fixture = Fixture::new();

        for name in ["my app", "My", "MY APP"] {
            let outcome = read_file(
                &fixture.db,
                &json!({ "project": name, "path": "src/auth.rs" }),
            );
            assert!(!outcome.is_error, "`{name}` should resolve: {}", outcome.content);
        }
    }

    #[test]
    fn an_unknown_project_lists_the_real_ones() {
        let fixture = Fixture::new();

        let outcome = read_file(
            &fixture.db,
            &json!({ "project": "Nonexistent", "path": "src/auth.rs" }),
        );

        assert!(outcome.is_error);
        assert!(outcome.content.contains("My App"), "got: {}", outcome.content);
    }

    #[test]
    fn reading_outside_the_project_folder_is_refused() {
        let fixture = Fixture::new();

        for escape in ["../../../etc/hosts", "/etc/hosts", "../secrets.txt"] {
            let outcome = read_file(
                &fixture.db,
                &json!({ "project": "My App", "path": escape }),
            );
            assert!(outcome.is_error, "`{escape}` should be refused");
        }
    }

    #[test]
    fn a_workspace_set_to_agent_is_still_only_readable_from_a_chat() {
        let fixture = Fixture::new();
        let id = fixture.db.list_workspaces().expect("list")[0].id.clone();
        fixture
            .db
            .update_workspace(&id, None, Some(WorkspaceMode::Agent), None)
            .expect("update");

        let record = &fixture.db.list_workspaces().expect("list")[0];

        assert_eq!(
            readable(record).mode,
            WorkspaceMode::Plan,
            "the Code page's mode is not consent for a chat to write"
        );
        assert!(!readable(record).permissions().access.allows_write());
    }

    #[test]
    fn searching_finds_matches_and_no_query_shows_the_layout() {
        let fixture = Fixture::new();

        let found = search_project(
            &fixture.db,
            &json!({ "project": "My App", "query": "sign_in" }),
        );
        assert!(!found.is_error, "{}", found.content);
        assert!(found.content.contains("src/auth.rs"));

        let layout = search_project(&fixture.db, &json!({ "project": "My App" }));
        assert!(!layout.is_error, "{}", layout.content);
        assert!(layout.content.contains("src/auth.rs"));
    }

    #[test]
    fn a_project_whose_folder_is_gone_says_so_rather_than_failing_obscurely() {
        let fixture = Fixture::new();
        std::fs::remove_dir_all(&fixture.root).expect("remove");

        let outcome = search_project(&fixture.db, &json!({ "project": "My App" }));

        assert!(outcome.is_error);
        assert!(outcome.content.contains("no longer on disk"));

        std::fs::create_dir_all(&fixture.root).ok();
    }
}
