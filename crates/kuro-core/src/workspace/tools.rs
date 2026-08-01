//! The tools a coding workspace offers.
//!
//! Nine, and the split between them is the permission ladder rather than a
//! taxonomy. Five are about the code as text — look around, read, search, change
//! a part, write a whole file — and four are about the code as a running thing:
//! run a command, start a server, look at what it is saying, stop it.
//!
//! The count matters. A small local model given twenty overlapping tools picks
//! badly and often picks nothing, so a mode never shows more than it needs:
//! Plan sees three, Agent and Bypass see all nine.
//!
//! Every file tool is scoped to the workspace root, and every one that changes a
//! file records what was there before so the changes panel can put it back. A
//! command is different in kind and is not pretended otherwise — see
//! [`super::exec`] for what is and is not contained about running one.

use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};

use super::process::ProcessRegistry;
use super::{exec, search, ToolRisk, Workspace, WorkspaceMode};
use crate::db::Db;
use crate::tools::{files, ToolOutcome, ToolSpec};

/// How long to wait for a just-started server to announce its address.
const SERVER_SETTLE: Duration = Duration::from_secs(12);
/// Lines of a server's output returned to the model at once.
const LOG_LINES: usize = 60;

/// A tool that acts on the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodingTool {
    ProjectTree,
    ReadFile,
    SearchFiles,
    EditFile,
    WriteFile,
    RunCommand,
    StartServer,
    CheckServer,
    StopServer,
}

impl CodingTool {
    pub const ALL: &'static [CodingTool] = &[
        Self::ProjectTree,
        Self::ReadFile,
        Self::SearchFiles,
        Self::EditFile,
        Self::WriteFile,
        Self::RunCommand,
        Self::StartServer,
        Self::CheckServer,
        Self::StopServer,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ProjectTree => "project_tree",
            Self::ReadFile => "read_file",
            Self::SearchFiles => "search_files",
            Self::EditFile => "edit_file",
            Self::WriteFile => "write_file",
            Self::RunCommand => "run_command",
            Self::StartServer => "start_server",
            Self::CheckServer => "check_server",
            Self::StopServer => "stop_server",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|tool| tool.name() == name)
    }

    pub fn risk(self) -> ToolRisk {
        match self {
            Self::ProjectTree | Self::ReadFile | Self::SearchFiles => ToolRisk::Read,
            Self::EditFile | Self::WriteFile => ToolRisk::Write,
            Self::RunCommand | Self::StartServer | Self::CheckServer | Self::StopServer => {
                ToolRisk::Execute
            }
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
            Self::RunCommand => {
                "Run a shell command in the project folder and wait for it to finish. This is \
                 how you check your work: build it, run the tests, run the type checker, run \
                 the linter. Use it for commands that end on their own — for a dev server, \
                 use start_server instead, or this will simply time out."
            }
            Self::StartServer => {
                "Start a long-running command in the background, such as a dev server, and \
                 return the address it printed. Use this when the user wants to see something \
                 running. The address is shown to them in a preview panel, so starting the \
                 server is what makes the page visible."
            }
            Self::CheckServer => {
                "Read the recent output of a background process, and whether it is still \
                 running. Call this after start_server to see whether it compiled, and after \
                 changing a file to see whether the rebuild succeeded."
            }
            Self::StopServer => {
                "Stop a background process you started. Do this when you are finished with it, \
                 or before starting a replacement on the same port."
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
            Self::RunCommand => json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description":
                            "The command line to run, exactly as you would type it. It runs \
                             in the project folder, so use relative paths.",
                    },
                    "timeout_seconds": {
                        "type": "integer",
                        "description":
                            "How long to wait before giving up. Defaults to 180. Raise it for \
                             a slow build, not for a command that never ends.",
                        "minimum": 1,
                        "maximum": 900,
                    },
                },
                "required": ["command"],
            }),
            Self::StartServer => json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description":
                            "The command that starts the server, such as `npm run dev`. It \
                             runs in the project folder.",
                    },
                },
                "required": ["command"],
            }),
            Self::CheckServer => json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The id start_server gave you.",
                    },
                },
                "required": ["id"],
            }),
            Self::StopServer => json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The id start_server gave you.",
                    },
                },
                "required": ["id"],
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
    /// Background processes belonging to the daemon, so a dev server started in
    /// one turn is still serving in the next.
    pub processes: &'a ProcessRegistry,
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
pub async fn run(
    tool: CodingTool,
    arguments: &Value,
    context: &WorkspaceContext<'_>,
) -> ToolOutcome {
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
        CodingTool::RunCommand => run_command(arguments, context).await,
        CodingTool::StartServer => run_start_server(arguments, context).await,
        CodingTool::CheckServer => run_check_server(arguments, context),
        CodingTool::StopServer => run_stop_server(arguments, context).await,
    }
}

/// Run a command and wait for it.
///
/// The vet happens here rather than at the boundary because the refusal has to
/// reach the *model*, not the user: told plainly that `terraform` is not on the
/// allowlist and that Bypass mode would run it, a model says so and asks. Told
/// nothing, it retries the same command three more times.
async fn run_command(arguments: &Value, context: &WorkspaceContext<'_>) -> ToolOutcome {
    let Some(command) = string_argument(arguments, "command") else {
        return ToolOutcome::failed("`command` is required and must be a string");
    };

    if let Err(refusal) = exec::vet(&command, context.workspace.mode.restricts_commands()) {
        return ToolOutcome::failed(refusal.to_string());
    }

    let timeout = arguments
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .map(Duration::from_secs)
        .unwrap_or(exec::DEFAULT_TIMEOUT);

    match exec::run(&context.workspace.root, &command, timeout).await {
        Ok(outcome) => {
            let described = outcome.describe();
            // A non-zero exit is reported as a failure so the model does not read
            // past it, but it is not a tool error: the tool did exactly what was
            // asked. Both facts are in the text.
            if outcome.succeeded() {
                ToolOutcome::ok(described)
            } else {
                ToolOutcome {
                    content: described,
                    is_error: true,
                    sources: Vec::new(),
                }
            }
        }
        Err(error) => ToolOutcome::failed(error),
    }
}

async fn run_start_server(arguments: &Value, context: &WorkspaceContext<'_>) -> ToolOutcome {
    let Some(command) = string_argument(arguments, "command") else {
        return ToolOutcome::failed("`command` is required and must be a string");
    };

    if let Err(refusal) = exec::vet(&command, context.workspace.mode.restricts_commands()) {
        return ToolOutcome::failed(refusal.to_string());
    }

    let started = match context
        .processes
        .start(&context.workspace.id, &context.workspace.root, &command)
        .await
    {
        Ok(started) => started,
        Err(error) => return ToolOutcome::failed(error),
    };

    // Waiting here is the difference between a useful answer and "started, no
    // address yet" on every single call.
    let settled = context
        .processes
        .settle(&started.id, SERVER_SETTLE)
        .await
        .unwrap_or(started);

    let log = context
        .processes
        .log(&settled.id, LOG_LINES)
        .unwrap_or_default()
        .join("\n");

    if !settled.running {
        return ToolOutcome {
            content: format!(
                "`{command}` exited immediately with code {}. It did not stay running, so \
                 there is nothing to preview. Its output:\n\n{log}",
                settled
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            is_error: true,
            sources: Vec::new(),
        };
    }

    match &settled.url {
        Some(url) => ToolOutcome::ok(format!(
            "`{command}` is running as process `{}` and is serving {url}. The user can see it \
             in the preview panel now. Its output so far:\n\n{log}",
            settled.id
        )),
        None => ToolOutcome::ok(format!(
            "`{command}` is running as process `{}`, but has not printed an address yet, so \
             there is nothing to preview. Call check_server with that id in a moment. Its \
             output so far:\n\n{log}",
            settled.id
        )),
    }
}

fn run_check_server(arguments: &Value, context: &WorkspaceContext<'_>) -> ToolOutcome {
    let Some(id) = string_argument(arguments, "id") else {
        return ToolOutcome::failed("`id` is required and must be a string");
    };

    let Some(found) = context.processes.get(&id) else {
        return ToolOutcome::failed(format!(
            "there is no process `{id}`. start_server returns the id to use here."
        ));
    };
    if found.workspace_id != context.workspace.id {
        return ToolOutcome::failed("that process belongs to another workspace");
    }

    let log = context
        .processes
        .log(&id, LOG_LINES)
        .unwrap_or_default()
        .join("\n");

    let status = if found.running {
        match &found.url {
            Some(url) => format!("`{}` is running and serving {url}.", found.command),
            None => format!("`{}` is running but has printed no address.", found.command),
        }
    } else {
        format!(
            "`{}` has stopped, with exit code {}.",
            found.command,
            found
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )
    };

    ToolOutcome::ok(format!("{status}\n\nRecent output:\n\n{log}"))
}

async fn run_stop_server(arguments: &Value, context: &WorkspaceContext<'_>) -> ToolOutcome {
    let Some(id) = string_argument(arguments, "id") else {
        return ToolOutcome::failed("`id` is required and must be a string");
    };

    let Some(found) = context.processes.get(&id) else {
        return ToolOutcome::failed(format!("there is no process `{id}`"));
    };
    if found.workspace_id != context.workspace.id {
        return ToolOutcome::failed("that process belongs to another workspace");
    }

    if context.processes.stop(&id) {
        ToolOutcome::ok(format!("Stopped `{}`.", found.command))
    } else {
        ToolOutcome::ok(format!("`{}` had already stopped.", found.command))
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
        processes: ProcessRegistry,
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
                processes: ProcessRegistry::new(),
            }
        }

        fn context(&self) -> WorkspaceContext<'_> {
            WorkspaceContext {
                db: &self.db,
                workspace: &self.workspace,
                conversation_id: None,
                processes: &self.processes,
            }
        }

        async fn run(&self, tool: CodingTool, arguments: Value) -> ToolOutcome {
            run(tool, &arguments, &self.context()).await
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
        assert!(
            !planning.contains(&CodingTool::RunCommand),
            "a command can change the project without touching a file tool"
        );

        assert_eq!(tools_for_mode(WorkspaceMode::Agent).len(), CodingTool::ALL.len());
        assert_eq!(tools_for_mode(WorkspaceMode::Bypass).len(), CodingTool::ALL.len());
    }

    #[tokio::test]
    async fn planning_refuses_a_write_even_when_the_tool_is_called_directly() {
        // The offered set is one filter; this is the other. A write must not
        // depend on a filter somewhere else having held.
        let fixture = Fixture::new(WorkspaceMode::Plan);

        let outcome = fixture
            .run(
                CodingTool::WriteFile,
                json!({ "path": "sneaky.txt", "content": "x" }),
            )
            .await;

        assert!(outcome.is_error, "{}", outcome.content);
        assert!(!fixture.workspace.root.join("sneaky.txt").exists());
    }

    #[tokio::test]
    async fn planning_refuses_to_run_anything_even_when_asked_directly() {
        let fixture = Fixture::new(WorkspaceMode::Plan);

        let outcome = fixture
            .run(CodingTool::RunCommand, json!({ "command": "echo hi" }))
            .await;

        assert!(outcome.is_error, "{}", outcome.content);
        assert!(outcome.content.contains("Plan mode"), "got: {}", outcome.content);
    }

    #[tokio::test]
    async fn reading_and_searching_work_in_plan_mode() {
        let fixture = Fixture::new(WorkspaceMode::Plan);

        let read = fixture
            .run(CodingTool::ReadFile, json!({ "path": "src/main.rs" }))
            .await;
        assert!(!read.is_error, "{}", read.content);
        assert!(read.content.contains("println!"));

        let found = fixture
            .run(CodingTool::SearchFiles, json!({ "query": "println" }))
            .await;
        assert!(found.content.contains("src/main.rs:2"), "got: {}", found.content);

        let tree = fixture.run(CodingTool::ProjectTree, json!({})).await;
        assert!(tree.content.contains("src/main.rs"));
    }

    #[tokio::test]
    async fn editing_replaces_one_snippet_and_leaves_the_rest_alone() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        let outcome = fixture
            .run(
                CodingTool::EditFile,
                json!({
                    "path": "src/main.rs",
                    "find": "println!(\"hi\")",
                    "replace": "println!(\"hello\")",
                }),
            )
            .await;

        assert!(!outcome.is_error, "{}", outcome.content);
        let after = std::fs::read_to_string(fixture.workspace.root.join("src/main.rs")).unwrap();
        assert!(after.contains("hello"));
        assert!(after.starts_with("fn main() {"), "the rest of the file must survive");
    }

    #[tokio::test]
    async fn an_ambiguous_edit_is_refused_rather_than_guessed_at() {
        let fixture = Fixture::new(WorkspaceMode::Agent);
        std::fs::write(fixture.workspace.root.join("src/dup.rs"), "let x = 1;\nlet x = 1;\n")
            .expect("write");

        let outcome = fixture
            .run(
                CodingTool::EditFile,
                json!({ "path": "src/dup.rs", "find": "let x = 1;", "replace": "let x = 2;" }),
            )
            .await;

        assert!(outcome.is_error);
        assert!(outcome.content.contains("2 times"), "got: {}", outcome.content);
        // Nothing was changed, so the model can retry with more context.
        let after = std::fs::read_to_string(fixture.workspace.root.join("src/dup.rs")).unwrap();
        assert_eq!(after, "let x = 1;\nlet x = 1;\n");
    }

    #[tokio::test]
    async fn an_edit_that_matches_nothing_tells_the_model_to_read_the_file() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        let outcome = fixture
            .run(
                CodingTool::EditFile,
                json!({ "path": "src/main.rs", "find": "not in the file", "replace": "x" }),
            )
            .await;

        assert!(outcome.is_error);
        assert!(outcome.content.contains("Read the file"), "got: {}", outcome.content);
    }

    #[tokio::test]
    async fn every_change_is_recorded_with_what_was_there_before() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        fixture
            .run(
                CodingTool::EditFile,
                json!({
                    "path": "src/main.rs",
                    "find": "println!(\"hi\")",
                    "replace": "println!(\"bye\")",
                }),
            )
            .await;
        fixture
            .run(
                CodingTool::WriteFile,
                json!({ "path": "src/new.rs", "content": "// fresh\n" }),
            )
            .await;

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

    #[tokio::test]
    async fn a_path_outside_the_workspace_is_refused() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        for escape in ["../escaped.txt", "/tmp/escaped.txt", "../../etc/hosts"] {
            let outcome = fixture
                .run(CodingTool::WriteFile, json!({ "path": escape, "content": "x" }))
                .await;
            assert!(outcome.is_error, "`{escape}` should be refused");
        }
    }

    #[tokio::test]
    async fn a_command_runs_in_the_project_and_reports_what_happened() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        let outcome = fixture
            .run(CodingTool::RunCommand, json!({ "command": "ls src" }))
            .await;

        assert!(!outcome.is_error, "{}", outcome.content);
        assert!(outcome.content.contains("main.rs"), "got: {}", outcome.content);
    }

    #[tokio::test]
    async fn a_failing_command_is_reported_as_a_failure_with_its_output() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        let outcome = fixture
            .run(
                CodingTool::RunCommand,
                json!({ "command": "echo 'boom' >&2; exit 2" }),
            )
            .await;

        assert!(outcome.is_error, "a non-zero exit is the answer, and it is a bad one");
        assert!(outcome.content.contains("exit code 2"), "got: {}", outcome.content);
        assert!(outcome.content.contains("boom"), "stderr matters most on a failure");
    }

    #[tokio::test]
    async fn agent_mode_refuses_a_command_outside_the_allowlist_and_says_what_would_allow_it() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        let outcome = fixture
            .run(CodingTool::RunCommand, json!({ "command": "terraform apply" }))
            .await;

        assert!(outcome.is_error);
        assert!(outcome.content.contains("Bypass"), "got: {}", outcome.content);
    }

    #[tokio::test]
    async fn bypass_mode_runs_it_but_still_refuses_the_things_nobody_meant() {
        let fixture = Fixture::new(WorkspaceMode::Bypass);

        let allowed = fixture
            .run(CodingTool::RunCommand, json!({ "command": "printf ok" }))
            .await;
        assert!(!allowed.is_error, "{}", allowed.content);

        let refused = fixture
            .run(CodingTool::RunCommand, json!({ "command": "sudo rm -rf /" }))
            .await;
        assert!(
            refused.is_error,
            "turning off the allowlist is not consent to reformat the machine"
        );
    }

    #[tokio::test]
    async fn a_server_is_started_tracked_and_stopped() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        let started = fixture
            .run(
                CodingTool::StartServer,
                json!({ "command": "echo http://localhost:5173 && sleep 20" }),
            )
            .await;
        assert!(!started.is_error, "{}", started.content);
        assert!(
            started.content.contains("http://localhost:5173"),
            "the address is what makes a preview possible; got: {}",
            started.content
        );

        let id = fixture.processes.list(&fixture.workspace.id)[0].id.clone();

        let checked = fixture
            .run(CodingTool::CheckServer, json!({ "id": id.clone() }))
            .await;
        assert!(!checked.is_error, "{}", checked.content);
        assert!(checked.content.contains("running"));

        let stopped = fixture
            .run(CodingTool::StopServer, json!({ "id": id }))
            .await;
        assert!(!stopped.is_error, "{}", stopped.content);
    }

    #[tokio::test]
    async fn a_server_that_dies_immediately_is_reported_rather_than_previewed() {
        let fixture = Fixture::new(WorkspaceMode::Agent);

        let outcome = fixture
            .run(
                CodingTool::StartServer,
                json!({ "command": "echo 'missing script'; exit 1" }),
            )
            .await;

        assert!(outcome.is_error, "{}", outcome.content);
        assert!(outcome.content.contains("nothing to preview"), "got: {}", outcome.content);
        assert!(outcome.content.contains("missing script"));
    }

    #[tokio::test]
    async fn a_process_from_another_workspace_cannot_be_reached() {
        let fixture = Fixture::new(WorkspaceMode::Agent);
        let elsewhere = fixture
            .processes
            .start("someone-elses-workspace", &std::env::temp_dir(), "sleep 20")
            .await
            .expect("started");

        let outcome = fixture
            .run(CodingTool::CheckServer, json!({ "id": elsewhere.id.clone() }))
            .await;

        assert!(outcome.is_error);
        assert!(outcome.content.contains("another workspace"));

        fixture.processes.stop(&elsewhere.id);
    }

    #[test]
    fn tools_declare_what_they_do_to_the_machine() {
        assert_eq!(CodingTool::ReadFile.risk(), ToolRisk::Read);
        assert_eq!(CodingTool::WriteFile.risk(), ToolRisk::Write);
        assert_eq!(CodingTool::RunCommand.risk(), ToolRisk::Execute);
        assert_eq!(describe_tools().len(), CodingTool::ALL.len());
        for tool in CodingTool::ALL {
            assert_eq!(CodingTool::parse(tool.name()), Some(*tool));
        }
    }
}
