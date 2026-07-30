//! HTTP surface.
//!
//! Two API families share one server:
//!
//! * `/api/*` is Kuro's own, covering models, downloads, conversations,
//!   hardware and settings.
//! * `/v1/*` is OpenAI-compatible, so existing tools work by changing a base
//!   URL.

use axum::routing::{delete, get, patch, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

pub mod chat;
pub mod common;
pub mod conversations;
pub mod downloads;
pub mod models;
pub mod openai;
pub mod settings;
pub mod system;

pub fn router(state: SharedState) -> Router {
    // The API is bound to loopback, and the browser UI is served from the same
    // origin. CORS exists only so the Vite dev server on another port can talk
    // to it during development.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .merge(native_routes())
        .merge(openai_routes())
        .merge(crate::static_files::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn native_routes() -> Router<SharedState> {
    Router::new()
        .route("/api/health", get(system::health))
        .route("/api/status", get(system::status))
        .route("/api/hardware", get(system::hardware))
        // Literal segments are matched ahead of `{id}`, so `/models/loaded`
        // and `/models/recommended` are not captured as model ids.
        .route("/api/models", get(models::list_models))
        .route("/api/models/recommended", get(models::recommended_models))
        .route("/api/models/loaded", get(models::loaded_models))
        .route("/api/models/pull", post(models::pull_model))
        .route("/api/models/preview", post(models::preview_pull))
        .route("/api/models/{id}", get(models::get_model))
        .route("/api/models/{id}", delete(models::delete_model))
        .route("/api/models/{id}/load", post(models::load_model))
        .route("/api/models/{id}/unload", post(models::unload_model))
        .route("/api/downloads", get(downloads::list_downloads))
        .route("/api/downloads/{id}", get(downloads::get_download))
        .route("/api/downloads/{id}/cancel", post(downloads::cancel_download))
        .route("/api/conversations", get(conversations::list_conversations))
        .route("/api/conversations", post(conversations::create_conversation))
        .route("/api/conversations/{id}", get(conversations::get_conversation))
        .route("/api/conversations/{id}", patch(conversations::update_conversation))
        .route("/api/conversations/{id}", delete(conversations::delete_conversation))
        .route("/api/conversations/{id}/messages", get(conversations::list_messages))
        .route("/api/conversations/{id}/messages", post(chat::send_message))
        .route("/api/settings", get(settings::get_settings))
        .route("/api/settings", patch(settings::patch_settings))
        .route("/api/settings/reset", post(settings::reset_settings))
}

fn openai_routes() -> Router<SharedState> {
    Router::new()
        .route("/v1/models", get(openai::list_models))
        .route("/v1/chat/completions", post(openai::chat_completions))
        .route("/v1/completions", post(openai::completions))
        .route("/v1/embeddings", post(openai::embeddings))
}
