use axum::extract::{Path, State};
use axum::Json;
use kuro_core::db::DownloadStatus;
use kuro_core::KuroError;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::state::SharedState;

pub async fn list_downloads(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    Ok(Json(json!({ "downloads": state.db.list_downloads()? })))
}

pub async fn get_download(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let record = state
        .db
        .get_download(&id)?
        .ok_or_else(|| KuroError::not_found(format!("download `{id}`")))?;
    Ok(Json(json!(record)))
}

/// Stop an in-flight transfer.
///
/// The partially downloaded file is kept so the same pull can resume later.
pub async fn cancel_download(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let record = state
        .db
        .get_download(&id)?
        .ok_or_else(|| KuroError::not_found(format!("download `{id}`")))?;

    if !record.status.is_active() {
        return Err(AppError(KuroError::bad_request(format!(
            "download is already {}",
            record.status.as_str()
        ))));
    }

    state.cancel_download(&id).await;
    state
        .db
        .set_download_status(&id, DownloadStatus::Cancelled, None)?;

    Ok(Json(json!({ "cancelled": true })))
}
