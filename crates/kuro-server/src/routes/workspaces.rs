//! Coding workspaces: the Code page's API.
//!
//! Everything that reads or changes a file on this machine goes through a
//! workspace, so this is the only route module that touches the user's own
//! folders. It is deliberately small — listing, reading, and putting a change
//! back — with the actual tool execution living in the chat turn.

use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::Json;
use kuro_core::db::UndoPlan;
use kuro_core::tools::files;
use kuro_core::workspace::{self, Workspace, WorkspaceMode};
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// Changes kept in the panel. Enough to cover a long session, few enough that
/// the response stays small.
const CHANGE_HISTORY: usize = 200;

pub async fn list_workspaces(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    Ok(Json(json!({
        "workspaces": state.db.list_workspaces()?,
        "modes": modes_json(),
        "tools": workspace::tools::describe_tools(),
    })))
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    pub name: String,
    #[serde(alias = "rootPath")]
    pub root_path: String,
}

/// Create a workspace over an existing folder.
///
/// The folder is checked here rather than at first use, so choosing one that is
/// not there is a mistake caught while the user is still looking at the form.
pub async fn create_workspace(
    State(state): State<SharedState>,
    Json(request): Json<CreateRequest>,
) -> AppResult<Json<Value>> {
    let expanded = expand_home(request.root_path.trim());

    if !expanded.exists() {
        return Err(KuroError::bad_request(format!(
            "`{}` does not exist.",
            request.root_path.trim()
        ))
        .into());
    }
    if !expanded.is_dir() {
        return Err(KuroError::bad_request(format!(
            "`{}` is a file. Choose the folder that contains it.",
            request.root_path.trim()
        ))
        .into());
    }

    // Stored canonicalised so that a workspace cannot be pointed at one folder
    // and resolved against another through a symlink that changes later.
    let root = expanded
        .canonicalize()
        .map_err(|error| KuroError::bad_request(format!("that folder could not be read: {error}")))?;

    let created = state
        .db
        .create_workspace(request.name.trim(), &root.to_string_lossy())?;

    // The stored default rather than the type's default: a new workspace should
    // open in whichever mode this person actually works in, and having to change
    // it on every new folder is the sort of friction that makes a setting feel
    // like a decoration.
    let preferred = kuro_core::settings::default_workspace_mode(&state.db)?;
    let created = if preferred == WorkspaceMode::parse(&created.mode).unwrap_or_default() {
        created
    } else {
        state
            .db
            .update_workspace(&created.id, None, Some(preferred), None)?
    };

    Ok(Json(json!(created)))
}

pub async fn get_workspace(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let workspace = load(&state, &id)?;
    Ok(Json(json!({
        "workspace": workspace,
        "conversations": state.db.list_workspace_conversations(&id)?,
    })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// `ask` | `plan` | `agent`.
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default, alias = "modelId")]
    pub model_id: Option<Option<String>>,
}

pub async fn update_workspace(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateRequest>,
) -> AppResult<Json<Value>> {
    let mode = match &request.mode {
        Some(raw) => Some(WorkspaceMode::parse(raw).ok_or_else(|| {
            KuroError::bad_request(format!("unknown mode `{raw}`. Use ask, plan or agent."))
        })?),
        None => None,
    };

    let updated = state.db.update_workspace(
        &id,
        request.name.as_deref(),
        mode,
        request.model_id.as_ref().map(|held| held.as_deref()),
    )?;
    Ok(Json(json!(updated)))
}

pub async fn delete_workspace(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    state.db.delete_workspace(&id)?;
    // Said plainly because "delete" on something describing a folder is a
    // reasonable thing to be nervous about.
    Ok(Json(json!({ "deleted": true, "folderUntouched": true })))
}

/// Start a chat inside a workspace.
pub async fn create_conversation(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let workspace = load(&state, &id)?;
    let conversation = state
        .db
        .create_conversation(workspace.model_id.as_deref())?;
    state
        .db
        .set_conversation_workspace(&conversation.id, Some(&id))?;

    let attached = state
        .db
        .get_conversation(&conversation.id)?
        .ok_or_else(|| KuroError::other("conversation vanished after creation"))?;
    Ok(Json(json!(attached)))
}

pub async fn workspace_tree(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let record = load(&state, &id)?;
    let root = PathBuf::from(&record.root_path);

    if !root.is_dir() {
        return Err(KuroError::not_found(format!("the folder `{}`", record.root_path)).into());
    }

    Ok(Json(json!({ "entries": workspace::search::tree(&root)? })))
}

#[derive(Debug, Deserialize)]
pub struct FileQuery {
    pub path: String,
}

/// Read one file for the viewer.
///
/// Goes through the same containment as the tools, at the read tier, so the
/// interface cannot reach anything a model could not.
pub async fn read_workspace_file(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<FileQuery>,
) -> AppResult<Json<Value>> {
    let workspace = as_workspace(&load(&state, &id)?, WorkspaceMode::Plan);
    let resolved = workspace.permissions().resolve_path(&query.path, false)?;
    let content = files::read_file(&resolved)?;

    Ok(Json(json!({ "path": query.path, "content": content })))
}

pub async fn list_changes(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    load(&state, &id)?;
    let changes = state.db.list_workspace_changes(&id, CHANGE_HISTORY)?;

    // `before`/`after` are whole file contents. Sending every one of them would
    // make this response many megabytes, so the list carries the shape of each
    // change and the contents stay server-side until an undo needs them.
    let summarised: Vec<Value> = changes
        .iter()
        .map(|change| {
            json!({
                "id": change.id,
                "path": change.path,
                "kind": change.kind,
                "conversationId": change.conversation_id,
                "createdAt": change.created_at,
                "undone": change.undone,
                "undoable": change.is_undoable(),
                "created": change.before.is_none(),
                "beforeLines": change.before.as_deref().map(count_lines),
                "afterLines": change.after.as_deref().map(count_lines),
            })
        })
        .collect();

    Ok(Json(json!({ "changes": summarised })))
}

/// One change, with the contents on both sides of it.
///
/// Separate from the list on purpose: the list is a summary because whole file
/// contents on every row would be megabytes, and this is the request that says
/// "that one, in full". Which is also the only shape a diff can take — a line
/// count tells you a file grew by nine lines and nothing about which nine.
pub async fn change_diff(
    State(state): State<SharedState>,
    Path((id, change_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    load(&state, &id)?;

    let change = state
        .db
        .list_workspace_changes(&id, CHANGE_HISTORY)?
        .into_iter()
        .find(|held| held.id == change_id)
        .ok_or_else(|| KuroError::not_found(format!("change `{change_id}`")))?;

    Ok(Json(json!({
        "id": change.id,
        "path": change.path,
        "kind": change.kind,
        "createdAt": change.created_at,
        "undone": change.undone,
        "created": change.before.is_none(),
        // Null rather than empty when the snapshot was skipped for size. The
        // difference matters: an empty string is a file that was empty, and a
        // viewer that showed the whole file as added would be lying about it.
        "before": change.before,
        "after": change.after,
    })))
}

/// Put a change back.
///
/// Refused when the file has been touched since, which is the property that
/// makes undo safe to offer: it can only ever remove a change the model made,
/// never one the user made afterwards.
pub async fn undo_change(
    State(state): State<SharedState>,
    Path((id, change_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let record = load(&state, &id)?;
    let change = state
        .db
        .get_workspace_change(&change_id)?
        .ok_or_else(|| KuroError::not_found(format!("change `{change_id}`")))?;

    if change.workspace_id != id {
        return Err(KuroError::bad_request("that change belongs to another workspace").into());
    }

    // Agent access, because undoing is a write. The path still goes through the
    // same containment as everything else.
    let workspace = as_workspace(&record, WorkspaceMode::Agent);
    let resolved = workspace.permissions().resolve_path(&change.path, true)?;
    let current = files::read_file(&resolved).unwrap_or_default();

    match change.plan_undo(&current) {
        UndoPlan::Restore(before) => {
            files::write_file(&resolved, &before)?;
        }
        UndoPlan::Remove => {
            std::fs::remove_file(&resolved).map_err(|error| {
                KuroError::other(format!("could not remove `{}`: {error}", change.path))
            })?;
        }
        UndoPlan::Refused(reason) => return Err(KuroError::bad_request(reason).into()),
    }

    state.db.mark_change_undone(&change_id)?;
    Ok(Json(json!({
        "undone": true,
        "path": change.path,
        "removed": change.before.is_none(),
    })))
}

/* ---------- Background processes ---------- */

/// Lines of a process's output returned at once.
const LOG_LINES: usize = 400;

/// What is running in this workspace, and what each one is serving.
///
/// The preview panel polls this. It is deliberately cheap — the registry is in
/// memory, and the log is read separately — because a panel that costs a
/// filesystem walk on every tick is a panel that gets a longer interval and then
/// feels dead.
pub async fn list_processes(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    load(&state, &id)?;
    Ok(Json(json!({ "processes": state.processes.list(&id) })))
}

#[derive(Debug, Deserialize)]
pub struct StartProcessRequest {
    pub command: String,
}

/// Start a command from the interface rather than from a model.
///
/// Vetted against the workspace's own mode, exactly as a model's call would be.
/// The user typing the command is not a reason to skip the check: the mode is
/// what they set, and a button that quietly outranked it would make the mode
/// meaningless.
pub async fn start_process(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<StartProcessRequest>,
) -> AppResult<Json<Value>> {
    let record = load(&state, &id)?;
    let mode = WorkspaceMode::parse(&record.mode).unwrap_or_default();

    if !mode.allows(kuro_core::workspace::ToolRisk::Execute) {
        return Err(KuroError::bad_request(format!(
            "{} mode does not run commands. Switch to Agent to run build and test commands.",
            mode.label()
        ))
        .into());
    }

    kuro_core::workspace::exec::vet(&request.command, mode.restricts_commands())
        .map_err(|refusal| KuroError::bad_request(refusal.to_string()))?;

    let root = PathBuf::from(&record.root_path);
    if !root.is_dir() {
        return Err(KuroError::not_found(format!("the folder `{}`", record.root_path)).into());
    }

    let started = state
        .processes
        .start(&id, &root, request.command.trim())
        .await
        .map_err(KuroError::bad_request)?;

    Ok(Json(json!({ "process": started })))
}

pub async fn process_log(
    State(state): State<SharedState>,
    Path((id, process_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    load(&state, &id)?;

    let found = state
        .processes
        .get(&process_id)
        .filter(|held| held.workspace_id == id)
        .ok_or_else(|| KuroError::not_found(format!("process `{process_id}`")))?;

    Ok(Json(json!({
        "process": found,
        "lines": state.processes.log(&process_id, LOG_LINES).unwrap_or_default(),
    })))
}

pub async fn stop_process(
    State(state): State<SharedState>,
    Path((id, process_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    load(&state, &id)?;

    let found = state
        .processes
        .get(&process_id)
        .filter(|held| held.workspace_id == id)
        .ok_or_else(|| KuroError::not_found(format!("process `{process_id}`")))?;

    let stopped = state.processes.stop(&process_id);
    Ok(Json(json!({ "stopped": stopped, "command": found.command })))
}

/// Take a finished process off the list.
///
/// The list used to be append-only for the lifetime of the daemon, so a morning
/// of builds left twenty dead rows above the one server actually running. A
/// still-running process is refused rather than dropped: forgetting one would
/// leave a bound port with nothing tracking it.
pub async fn forget_process(
    State(state): State<SharedState>,
    Path((id, process_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    load(&state, &id)?;

    let found = state
        .processes
        .get(&process_id)
        .filter(|held| held.workspace_id == id)
        .ok_or_else(|| KuroError::not_found(format!("process `{process_id}`")))?;

    if found.running {
        return Err(KuroError::other(format!(
            "`{}` is still running. Stop it before clearing it.",
            found.command
        ))
        .into());
    }

    Ok(Json(json!({ "forgotten": state.processes.forget(&process_id) })))
}

/// Take every finished process off one workspace's list.
pub async fn clear_processes(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    load(&state, &id)?;
    Ok(Json(json!({ "cleared": state.processes.forget_finished(&id) })))
}

fn load(state: &SharedState, id: &str) -> Result<kuro_core::db::WorkspaceRecord, KuroError> {
    state
        .db
        .get_workspace(id)?
        .ok_or_else(|| KuroError::not_found(format!("workspace `{id}`")))
}

/// The enforcement object for a stored workspace, at a chosen mode.
///
/// The mode is passed rather than read from the record because the interface's
/// own reads and undos are not the model's: viewing a file works in any mode,
/// and undo is a write whatever the workspace is currently set to.
fn as_workspace(record: &kuro_core::db::WorkspaceRecord, mode: WorkspaceMode) -> Workspace {
    Workspace {
        id: record.id.clone(),
        root: PathBuf::from(&record.root_path),
        mode,
    }
}

fn modes_json() -> Vec<Value> {
    WorkspaceMode::ALL
        .iter()
        .map(|mode| {
            json!({
                "id": mode.as_str(),
                "label": mode.label(),
                "blurb": mode.blurb(),
                "tools": workspace::tools::tools_for_mode(*mode)
                    .iter()
                    .map(|tool| tool.name())
                    .collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn count_lines(text: &str) -> usize {
    text.lines().count()
}

/// Expand a leading `~`, so a typed path matches what the tools will use.
fn expand_home(raw: &str) -> PathBuf {
    let Some(rest) = raw.strip_prefix('~') else {
        return PathBuf::from(raw);
    };
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return PathBuf::from(raw);
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home
    } else {
        home.join(rest)
    }
}
