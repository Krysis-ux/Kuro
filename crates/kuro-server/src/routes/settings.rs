use axum::extract::State;
use axum::Json;
use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};
use crate::state::SharedState;

/// Every stored setting.
///
/// Keys the frontend owns (display toggles, composer preferences) live here
/// alongside the handful the backend reads, so the UI needs no backend change
/// to add a preference.
pub async fn get_settings(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    Ok(Json(Value::Object(state.db.all_settings()?)))
}

/// Merge a partial settings object. Sending `null` for a key removes it.
pub async fn patch_settings(
    State(state): State<SharedState>,
    Json(payload): Json<Value>,
) -> AppResult<Json<Value>> {
    let Value::Object(updates) = payload else {
        return Err(AppError(kuro_core::KuroError::bad_request(
            "settings must be a JSON object",
        )));
    };

    for (key, value) in updates {
        if value.is_null() {
            state.db.delete_setting(&key)?;
        } else {
            state.db.set_setting(&key, &value)?;
        }
    }

    Ok(Json(Value::Object(state.db.all_settings()?)))
}

/// Restore defaults by clearing every stored override.
pub async fn reset_settings(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    for key in state.db.all_settings()?.keys() {
        state.db.delete_setting(key)?;
    }
    Ok(Json(Value::Object(Map::new())))
}
