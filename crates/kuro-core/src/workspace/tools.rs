//! The tools a coding workspace offers.
//!
//! Five, deliberately. A small local model given twenty overlapping tools picks
//! badly and often picks nothing; five that map onto the things anyone actually
//! does to a codebase — look around, read, search, change a part, write a whole
//! file — is a set a 4B model can use.
//!
//! Every tool is scoped to the workspace root, and every one that changes a file
//! records what was there before, so the changes panel can put it back.

use serde::Serialize;
use serde_json::{json, Value};

use super::{search, ToolRisk, Workspace, WorkspaceMode};
use crate::db::Db;
use crate::tools::{files, ToolOutcome, ToolSpec};

/// A tool that acts on the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodingTool {
    ProjectTree,
    ReadFile,
    SearchFiles,
    EditFile,
    WriteFile,
}

impl CodingTool {
    pub const ALL: &'static [CodingTool] = &[
        Self::ProjectTree,
        Self::ReadFile,
        Self::SearchFiles,
        Self::EditFile,
        Self::WriteFile,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ProjectTree => "project_tree",
            Self::ReadFile => "read_file",
            Self::SearchFiles => "search_files",
            Self::EditFile => "edit_file",
            Self::WriteFile => "write_file",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|tool| tool.name() == name)
    }

    pub fn risk(self) -> ToolRisk {
        match self {
            Self::ProjectTree | Self::ReadFile | Self::SearchFiles => ToolRisk::Read,
            Self::EditFile | Self::WriteFile => ToolRisk::Write,
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::ProjectTree => {
                "List the files in this project. Call this first when you do not already know \
                 the layout. Dependency and build directories are left out."
            }
            Self::ReadFile => {
                "Read a file from this project. Always read a file before changing it — never \
                 edit from memory of what it probably contains."
            }
            Self::SearchFiles => {
                "Find a literal string across the project. Use this to locate a function, an \
                 import, or every place something is used. Returns paths with line numbers."
            }
            Self::EditFile => {
                "Change one part of a file, by replacing an exact snippet with a new one. \
                 Prefer this over write_file for any file that already exists: it leaves the \
                 rest of the file alone. The snippet in `find` must appear exactly once, so \
                 include enough surrounding lines to make it unique."
            }
            Self::WriteFile => {
                "Create a new file, or replace an existing one entirely. Use this for new \
                 files. For a file that already exists, prefer edit_file — this replaces the \
                 whole thing, and everything not in `content` is gone."
            }
        }
    }

    fn parameters(self) -> Value {
        match self {
            Self::ProjectTree => json!({
                "type": "object",
                "properties": {},
            }),
            Self::ReadFile => json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the project root, such as src/main.rs.",
                    },
                },
                "required": ["path"],
            }),
            Self::SearchFiles => json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The exact text to look for. Not a regular expression.",
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Match case exactly. Defaults to false.",
                    },
                },
                "required": ["query"],
            }),
            Self::EditFile => json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the project root.",
                    },
                    "find": {
                        "type": "string",
                        "description":
                            "The exact snippet to replace, copied from the file. Must match \
                             once, including indentation.",
                    },
                    "replace": {
                        "type": "string",
                        "description": "What to put in its place.",
                    },
                },
                "required": ["path", "find", "replace"],
            }),
            Self::WriteFile => json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the project root.",
                    },
                    "content": {
                        "type": "string",
                        "description": "The complete contents of the file.",
                    },
                },
                "required": ["path", "content"],
            }),
        }
    }

    pub fn spec(self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
            origin: crate::tools::ToolOrigin::Builtin,
        }
    }
}

/// Which tools a mode offers.
///
/// Filtered here rather than refused at call time: a model that is never shown
/// `write_file` cannot decide to try it, which is both a stronger guarantee and
/// a much easier one to explain than a call that fails afterwards.
pub fn tools_for_mode(mode: WorkspaceMode) -> Vec<CodingTool> {
    CodingTool::ALL
        .iter()
        .copied()
        .filter(|tool| mode.allows(tool.risk()))
        .collect()
}

/// What the coding tools need in order to run.
pub struct WorkspaceContext<'a> {
    pub db: &'a Db,
    pub workspace: &'a Workspace,
    /// Recorded against each change so the panel can say which turn made it.
    pub conversation_id: Option<&'a str>,
}

/// Everything the interface needs to describe a tool.
#[derive(Debug, Clone, Serialize)]
pub struct CodingToolDescription {
    pub name: String,
    pub description: String,
    pub risk: ToolRisk,
    pub risk_label: String,
}

pub fn describe_tools() -> Vec<CodingToolDescription> {
    CodingTool::ALL
        .iter()
        .map(|tool| CodingToolDescription {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            risk: tool.risk(),
            risk_label: tool.risk().label().to_string(),
        })
        .collect()
}

/// Run one coding tool.
pub fn run(tool: CodingTool, arguments: &Value, context: &WorkspaceContext<'_>) -> ToolOutcome {
    // Checked again here even though the tool was filtered out of the offered
    // set. The two paths have different inputs — one the mode, one the model's
    // chosen name — and a write must not depend on a filter elsewhere holding.
    if !context.workspace.mode.allows(tool.risk()) {
        return ToolOutcome::failed(format!(
            "`{}` is not available in {} mode",
            tool.name(),
            context.workspace.mode.label()
        ));
    }
    if !context.workspace.root_exists() {
        return ToolOutcome::failed(format!(
            "the project folder `{}` no longer exists",
            context.workspace.root.display()
        ));
    }

    match tool {
        CodingTool::ProjectTree => run_tree(context),
        CodingTool::ReadFile => run_read(arguments, context),
        CodingTool::SearchFiles => run_search(arguments, context),
        CodingTool::EditFile => run_edit(arguments, context),
        CodingTool::WriteFile => run_write(arguments, context),
    }
}

fn run_tree(context: &WorkspaceContext<'_>) -> ToolOutcome {
    match search::tree(&context.workspace.root) {
        Ok(entries) => ToolOutcome::ok(search::format_tree(&context.workspace.root, &entries)),
        Err(error) => ToolOutcome::failed(error),
    }
}

fn run_read(arguments: &Value, context: &WorkspaceContext<'_>) -> ToolOutcome {
    let Some(path) = string_argument(arguments, "path") else {
        return ToolOutcome::failed("`path` is required and must be a string");
    };

    match context.workspace.permissions().resolve_path(&path, false) {
        Ok(resolved) => match files::read_file(&resolved) {
            Ok(text) => ToolOutcome::ok(format!("`{path}`:\n\n{text}")),
            Err(error) => ToolOutcome::failed(error),
        },
        Err(error) => ToolOutcome::failed(error),
    }
}

fn run_search(arguments: &Value, context: &WorkspaceContext<'_>) -> ToolOutcome {
    let Some(query) = string_argument(arguments, "query") else {
        return ToolOutcome::failed("`query` is required and must be a string");
    };
    let case_sensitive = arguments
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    match search::find_text(&context.workspace.root, &query, case_sensitive) {
        Ok(found) => ToolOutcome::ok(search::format_matches(&query, &found)),
        Err(error) => ToolOutcome::failed(error),
    }
}

/// Replace an exact snippet.
///
/// The uniqueness requirement is the whole safety property. A `find` that occurs
/// twice is ambiguous, and picking one is how an agent silently changes the
/// wrong call site; the model is told to include more context instead. A `find`
/// that occurs zero times almost always means the model is editing from memory,
/// and saying so plainly gets a re-read rather than a retry of the same guess.
fn run_edit(arguments: &Value, context: &WorkspaceContext<'_>) -> ToolOutcome {
    let Some(path) = string_argument(arguments, "path") else {
        return ToolOutcome::failed("`path` is required and must be a string");
    };
    let Some(find) = arguments.get("find").and_then(Value::as_str) else {
        return ToolOutcome::failed("`find` is required and must be a string");
    };
    let Some(replace) = arguments.get("replace").and_then(Value::as_str) else {
        return ToolOutcome::failed("`replace` is required and must be a string");
    };
    if find.is_empty() {
        return ToolOutcome::failed("`find` must not be empty. Use write_file to create a file.");
    }

    let permissions = context.workspace.permissions();
    let resolved = match permissions.resolve_path(&path, true) {
        Ok(resolved) => resolved,
        Err(error) => return ToolOutcome::failed(error),
    };

    let before = match files::read_file(&resolved) {
        Ok(text) => text,
        Err(error) => return ToolOutcome::failed(error),
    };

    let occurrences = before.matches(find).count();
    match occurrences {
        0 => {
            return ToolOutcome::failed(format!(
                "that snippet does not appear in `{path}`. Read the file and copy the text \
                 exactly, including indentation."
            ))
        }
        1 => {}
        many => {
            return ToolOutcome::failed(format!(
                "that snippet appears {many} times in `{path}`, so it is ambiguous. Include \
                 more surrounding lines so it matches exactly once."
            ))
        }
    }

    let after = before.replacen(find, replace, 1);

    match files::write_file(&resolved, &after) {
        Ok(_) => {
            record(context, &path, Some(&before), &after, "edit");
            let delta = after.lines().count() as i64 - before.lines().count() as i64;
            ToolOutcome::ok(format!(
                "Edited `{path}` ({} lines, {}{delta} from the change).",
                after.lines().count(),
                if delta >= 0 { "+" } else { "" }
            ))
        }
        Err(error) => ToolOutcome::failed(error),
    }
}

fn run_write(arguments: &Value, context: &WorkspaceContext<'_>) -> ToolOutcome {
    let Some(path) = string_argument(arguments, "path") else {
        return ToolOutcome::failed("`path` is required and must be a string");
    };
    // An empty file is a legitimate thing to write, so this is read directly
    // rather than through the blank-rejecting helper.
    let Some(content) = arguments.get("content").and_then(Value::as_str) else {
        return ToolOutcome::failed("`content` is required and must be a string");
    };

    let permissions = context.workspace.permissions();
    let resolved = match permissions.resolve_path(&path, true) {
        Ok(resolved) => resolved,
        Err(error) => return ToolOutcome::failed(error),
    };

    // Read before writing so an overwrite can be undone. A file that does not
    // exist yet reads as absent, which is what marks the change as a creation.
    let before = files::read_file(&resolved).ok();

    match files::write_file(&resolved, content) {
        Ok(report) => {
            record(context, &path, before.as_deref(), content, "write");
            ToolOutcome::ok(report.describe(&resolved))
        }
        Err(error) => ToolOutcome::failed(error),
    }
}

/// Record a change so it can be undone.
///
/// Best effort: a change that has already happened on disk must still be
/// reported to the model even if the log write fails, because telling it the
/// edit failed would send it round again on a file that is already edited.
fn record(
    context: &WorkspaceContext<'_>,
    path: &str,
    before: Option<&str>,
    after: &str,
    kind: &str,
) {
    if let Err(error) = context.db.record_workspace_change(
        &context.workspace.id,
        context.conversation_id,
        path,
        kind,
        before,
        after,
    ) {
        tracing::warn!(%error, path, "a workspace change could not be recorded for undo");
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

    struct Fixture {
        db: Db,
        workspace: Workspace,
    }

    impl Fixture {
        fn new(mode: WorkspaceMode) -> Self {
            let root = std::env::temp_dir().join(format!("kuro-tools-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(root.join("src")).expect("mkdir");
            std::fs::write(root.join("src/main.rs"), "fn main() {\n    println!(\"hi\");\n}\n")
                .expect("write");

            let db = Db::open_in_memory().expect("open");
            let record = db
                .create_workspace("Sample", &root.to_string_lossy())
                .expect("workspace");

            Self {
                db,
                workspace: Workspace {
                    id: record.id,
                    root,
                    mode,
                },
            }
        }

        fn context(&self) -> WorkspaceContext<'_> {
            WorkspaceContext {
                db: &self.db,
                workspace: &self.workspace,
                conversation_id: None,
            }
        }

        fn run(&self, tool: CodingTool, arguments: Value) -> ToolOutcome {
            run(tool, &arguments, &self.context())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.workspace.root).ok();
        }
    }

    #[test]
    fn each_mode_offers_the_tools_it_permits() {
        assert!(tools_for_mode(WorkspaceMode::Ask).is_empty());

        let planning = tools_for_mode(WorkspaceMode::Plan);
        assert!(planning.contains(&CodingTool::ReadFile));
        assert!(planning.contains(&CodingTool::SearchFiles));
        assert!(
            !planning.contains(&CodingTool::WriteFile),
            "a model cannot try what it was never shown"
        );
        assert!(!planning.contains(&CodingTool::EditFile));

        assert_eq!(tools_for_mode(WorkspaceMode::Agent).len(), CodingTool::ALL.len());
    }

    #[test]
    fn planning_refuses_a_write_even_when_the_tool_is_called_directly() {
        // The offered set is one filter; this is the other. A write must not
        // depend on a filter somewhere else having held.
        let fixture = Fixture::new(WorkspaceMode::Plan);

        let outcome = fixture.run(
            CodingTool::WriteFile,
            json!({ "path": "sneaky.txt", "content": "x" }),
        );

        assert!(outcome.is_error, "{}", outcome.content);
        assert!(!fixture.workspace.root.join("sneaky.txt").exists());
    }

    #[test]
    fn reading_and_searching_work_in_plan_mode() {
        let fixture = Fixture::new(WorkspaceMode::Plan);

        let read = fixture.run(CodingTool::ReadFile, json!({ "path": "src/main.rs" }));
        assert!(!read.is_error, "{}", read.content);
        assert!(read.content.contains("println!"));

        let found = fixture.run(CodingTool::SearchFiles, json!({ "query": "println" }));
        assert!(found.content.contains("src/main.rs:2"), "got: {}", found.content);

        let tree = fixture.run(CodingTool::ProjectTree, json!({}));
        assert!(tree.content.contains("src/main.rs"));
    }

    #[test]
    fn editing_replaces_one_snippet_and_leaves_the_rest_alone() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        let outcome = fixture.run(
            CodingTool::EditFile,
            json!({
                "path": "src/main.rs",
                "find": "println!(\"hi\")",
                "replace": "println!(\"hello\")",
            }),
        );

        assert!(!outcome.is_error, "{}", outcome.content);
        let after = std::fs::read_to_string(fixture.workspace.root.join("src/main.rs")).unwrap();
        assert!(after.contains("hello"));
        assert!(after.starts_with("fn main() {"), "the rest of the file must survive");
    }

    #[test]
    fn an_ambiguous_edit_is_refused_rather_than_guessed_at() {
        let fixture = Fixture::new(WorkspaceMode::Agent);
        std::fs::write(fixture.workspace.root.join("src/dup.rs"), "let x = 1;\nlet x = 1;\n")
            .expect("write");

        let outcome = fixture.run(
            CodingTool::EditFile,
            json!({ "path": "src/dup.rs", "find": "let x = 1;", "replace": "let x = 2;" }),
        );

        assert!(outcome.is_error);
        assert!(outcome.content.contains("2 times"), "got: {}", outcome.content);
        // Nothing was changed, so the model can retry with more context.
        let after = std::fs::read_to_string(fixture.workspace.root.join("src/dup.rs")).unwrap();
        assert_eq!(after, "let x = 1;\nlet x = 1;\n");
    }

    #[test]
    fn an_edit_that_matches_nothing_tells_the_model_to_read_the_file() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        let outcome = fixture.run(
            CodingTool::EditFile,
            json!({ "path": "src/main.rs", "find": "not in the file", "replace": "x" }),
        );

        assert!(outcome.is_error);
        assert!(outcome.content.contains("Read the file"), "got: {}", outcome.content);
    }

    #[test]
    fn every_change_is_recorded_with_what_was_there_before() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        fixture.run(
            CodingTool::EditFile,
            json!({
                "path": "src/main.rs",
                "find": "println!(\"hi\")",
                "replace": "println!(\"bye\")",
            }),
        );
        fixture.run(
            CodingTool::WriteFile,
            json!({ "path": "src/new.rs", "content": "// fresh\n" }),
        );

        let changes = fixture
            .db
            .list_workspace_changes(&fixture.workspace.id, 50)
            .expect("changes");

        assert_eq!(changes.len(), 2);
        // Newest first.
        assert_eq!(changes[0].path, "src/new.rs");
        assert_eq!(changes[0].kind, "write");
        assert!(changes[0].before.is_none(), "a new file has nothing before it");
        assert_eq!(changes[1].path, "src/main.rs");
        assert!(changes[1].before.as_deref().unwrap().contains("hi"));
    }

    #[test]
    fn a_path_outside_the_workspace_is_refused() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        for escape in ["../escaped.txt", "/tmp/escaped.txt", "../../etc/hosts"] {
            let outcome =
                fixture.run(CodingTool::WriteFile, json!({ "path": escape, "content": "x" }));
            assert!(outcome.is_error, "`{escape}` should be refused");
        }
    }

    #[test]
    fn tools_declare_what_they_do_to_the_machine() {
        assert_eq!(CodingTool::ReadFile.risk(), ToolRisk::Read);
        assert_eq!(CodingTool::WriteFile.risk(), ToolRisk::Write);
        assert_eq!(describe_tools().len(), CodingTool::ALL.len());
        for tool in CodingTool::ALL {
            assert_eq!(CodingTool::parse(tool.name()), Some(*tool));
        }
    }
}
