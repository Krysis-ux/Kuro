use axum::extract::{Path, Query, State};
use axum::Json;
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Free-text search across titles and message bodies.
    #[serde(default)]
    pub q: Option<String>,
}

pub async fn list_conversations(
    State(state): State<SharedState>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    let conversations = state.db.list_conversations(query.q.as_deref())?;
    Ok(Json(json!({ "conversations": conversations })))
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    #[serde(default)]
    pub model_id: Option<String>,
}

pub async fn create_conversation(
    State(state): State<SharedState>,
    Json(request): Json<CreateRequest>,
) -> AppResult<Json<Value>> {
    let conversation = state.db.create_conversation(request.model_id.as_deref())?;
    Ok(Json(json!(conversation)))
}

pub async fn get_conversation(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let conversation = state
        .db
        .get_conversation(&id)?
        .ok_or_else(|| KuroError::not_found(format!("conversation `{id}`")))?;
    Ok(Json(json!(conversation)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub archived: Option<bool>,
}

pub async fn update_conversation(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateRequest>,
) -> AppResult<Json<Value>> {
    state
        .db
        .get_conversation(&id)?
        .ok_or_else(|| KuroError::not_found(format!("conversation `{id}`")))?;

    if let Some(title) = &request.title {
        // A title set through the API is the user's choice, so mark it manual
        // and stop automatic titling from overwriting it.
        state.db.set_conversation_title(&id, title, true)?;
    }
    if let Some(model_id) = &request.model_id {
        state.db.set_conversation_model(&id, model_id)?;
    }
    if request.pinned.is_some() || request.archived.is_some() {
        state
            .db
            .set_conversation_flags(&id, request.pinned, request.archived)?;
    }

    let conversation = state
        .db
        .get_conversation(&id)?
        .ok_or_else(|| KuroError::not_found(format!("conversation `{id}`")))?;
    Ok(Json(json!(conversation)))
}

pub async fn delete_conversation(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    state.db.delete_conversation(&id)?;
    Ok(Json(json!({ "deleted": true })))
}

pub async fn list_messages(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    state
        .db
        .get_conversation(&id)?
        .ok_or_else(|| KuroError::not_found(format!("conversation `{id}`")))?;

    Ok(Json(json!({ "messages": state.db.list_messages(&id)? })))
}
