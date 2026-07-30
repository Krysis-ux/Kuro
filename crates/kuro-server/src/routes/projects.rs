//! Projects.
//!
//! Standing instructions plus a grouping of conversations. The value is the
//! instructions — said once, applied to every chat in the project — and the
//! grouping is what makes them findable again later.

use axum::extract::{Path, State};
use axum::Json;
use kuro_core::db::{NewProject, ProjectUpdate};
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

pub async fn list_projects(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    Ok(Json(json!({ "projects": state.db.list_projects()? })))
}

/// One project with the conversations inside it.
pub async fn get_project(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let project = state
        .db
        .get_project(&id)?
        .ok_or_else(|| KuroError::not_found(format!("project `{id}`")))?;

    let conversations = state.db.list_project_conversations(&id)?;

    Ok(Json(json!({ "project": project, "conversations": conversations })))
}

pub async fn create_project(
    State(state): State<SharedState>,
    Json(input): Json<NewProject>,
) -> AppResult<Json<Value>> {
    Ok(Json(json!({ "project": state.db.insert_project(&input)? })))
}

pub async fn update_project(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(patch): Json<ProjectUpdate>,
) -> AppResult<Json<Value>> {
    Ok(Json(json!({ "project": state.db.update_project(&id, &patch)? })))
}

/// Delete a project. Its conversations are released, not deleted.
pub async fn delete_project(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let removed = state.db.delete_project(&id)?;
    if !removed {
        return Err(KuroError::not_found(format!("project `{id}`")).into());
    }
    Ok(Json(json!({ "deleted": true })))
}

#[derive(Debug, Deserialize)]
pub struct MoveRequest {
    /// `null` moves the conversation out of any project.
    #[serde(rename = "projectId")]
    pub project_id: Option<String>,
}

pub async fn move_conversation(
    State(state): State<SharedState>,
    Path(conversation_id): Path<String>,
    Json(request): Json<MoveRequest>,
) -> AppResult<Json<Value>> {
    state
        .db
        .set_conversation_project(&conversation_id, request.project_id.as_deref())?;

    Ok(Json(json!({ "projectId": request.project_id })))
}
