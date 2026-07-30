use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use kuro_core::KuroError;
use serde_json::json;

/// HTTP wrapper around [`KuroError`].
///
/// The error body follows OpenAI's shape so that clients pointed at Kuro's
/// compatible endpoints can parse failures with the code they already have.
pub struct AppError(pub KuroError);

impl From<KuroError> for AppError {
    fn from(error: KuroError) -> Self {
        Self(error)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        Self(KuroError::Http(error))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self(KuroError::Json(error))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, kind) = match &self.0 {
            KuroError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            KuroError::BadRequest(_) => (StatusCode::BAD_REQUEST, "invalid_request_error"),
            KuroError::Model(_) => (StatusCode::BAD_REQUEST, "model_error"),
            KuroError::Engine(_) => (StatusCode::SERVICE_UNAVAILABLE, "engine_error"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        // Server-side faults are logged in full; the client gets the message.
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self.0, "request failed");
        }

        let body = Json(json!({
            "error": {
                "message": self.0.to_string(),
                "type": kind,
            }
        }));

        (status, body).into_response()
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;
