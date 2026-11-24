//! HTTP surface. Two API families share one server:
//!
//! * `/api/*` is Kuro's own, used by the web UI, the CLI and the macOS app.
//! * `/v1/*` is OpenAI- and Anthropic-compatible, so existing tools work by
//!   changing a base URL.

use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

pub mod anthropic;
pub mod automations;
pub mod browse;
pub mod chat;
pub mod common;
pub mod conversations;
pub mod downloads;
pub mod free;
pub mod mcp;
pub mod models;
pub mod openai;
pub mod projects;
pub mod providers;
pub mod settings;
pub mod subagent;
pub mod system;
pub mod tools;
pub mod tools_runtime;
pub mod workspaces;

/// The port the Vite dev server runs on, and the only other origin allowed.
const DEV_SERVER_PORT: u16 = 5173;

pub fn router(state: SharedState) -> Router {
    let cors = cors_layer(state.port);

    Router::new()
        .merge(native_routes())
        .merge(openai_routes())
        .merge(crate::static_files::router())
        .layer(cors)
        .layer(middleware::from_fn_with_state(state.port, same_machine_only))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// CORS allow-list: this server's own origin and the Vite dev server, nothing else.
///
/// There is no authentication and the API includes `POST
/// /api/workspaces/{id}/processes` (runs a command), so a permissive policy
/// would let any web page the user visits list workspaces and execute commands.
/// Other origins get no `Access-Control-Allow-Origin` header, so the browser
/// blocks both the preflight and the read.
///
/// CORS only constrains browsers and is not authentication: other local programs
/// are unaffected, and a page can still fire a "simple" cross-origin request
/// whose reply it cannot read. The real fix is a key on every request.
fn cors_layer(port: u16) -> CorsLayer {
    let origins: Vec<HeaderValue> = allowed_origins(port)
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        // Only what Kuro's own client sends; `Any` would let a page name any header in a preflight.
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT])
}

/// This server's own origin and the Vite dev server. Both loopback spellings,
/// since a browser treats them as different origins.
fn allowed_origins(port: u16) -> [String; 4] {
    [
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{DEV_SERVER_PORT}"),
        format!("http://localhost:{DEV_SERVER_PORT}"),
    ]
}

/// Refuse what a web page in the user's browser could send, before any handler runs.
///
/// CORS stops a page reading replies, not sending requests: a body-less `POST
/// /api/shutdown` needs no preflight. So a request carrying another site's
/// `Origin` is refused outright. A `Host` other than loopback is refused too: that
/// is DNS rebinding, a page whose own name now resolves to 127.0.0.1, which the
/// browser treats as same-origin. The app, the CLI and API clients send no
/// `Origin` and a loopback `Host`, so they are unaffected.
async fn same_machine_only(State(port): State<u16>, request: Request, next: Next) -> Response {
    // Scoped so no borrow of the request is held across the await below.
    let local = {
        let header = |name| request.headers().get(name).and_then(|value| value.to_str().ok());
        is_local_request(header(header::HOST), header(header::ORIGIN), port)
    };
    if local {
        return next.run(request).await;
    }
    let message = "Kuro only answers this machine's own apps and its own pages.";
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": { "message": message, "type": "forbidden" } })))
        .into_response()
}

fn is_local_request(host: Option<&str>, origin: Option<&str>, port: u16) -> bool {
    let host_is_loopback = host.is_none_or(|host| {
        let name = match host.strip_prefix('[') {
            Some(rest) => rest.split(']').next().unwrap_or_default(),
            None => host.rsplit_once(':').map_or(host, |(name, _)| name),
        };
        matches!(name.to_ascii_lowercase().as_str(), "127.0.0.1" | "localhost" | "::1")
    });
    let origin_is_ours = origin.is_none_or(|origin| allowed_origins(port).iter().any(|allowed| allowed == origin));
    host_is_loopback && origin_is_ours
}

fn native_routes() -> Router<SharedState> {
    Router::new()
        .route("/api/health", get(system::health))
        .route("/api/status", get(system::status))
        .route("/api/hardware", get(system::hardware))
        .route("/api/shutdown", post(system::shutdown))
        .route("/api/restart", post(system::restart))
        // Literal segments match ahead of `{id}`, so `/models/loaded` is not taken as a model id.
        .route("/api/models", get(models::list_models))
        .route("/api/models/recommended", get(models::recommended_models))
        .route("/api/models/loaded", get(models::loaded_models))
        .route("/api/models/search", get(models::search_hub))
        .route("/api/models/details", get(models::hub_details))
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
        // Edit truncates at the message and re-answers; fork copies up to it, leaving the original.
        .route(
            "/api/conversations/{id}/messages/{message_id}",
            patch(chat::edit_message),
        )
        .route("/api/conversations/{id}/fork", post(conversations::fork_conversation))
        .route("/api/conversations/{id}/project", post(projects::move_conversation))
        .route("/api/projects", get(projects::list_projects))
        .route("/api/projects", post(projects::create_project))
        .route("/api/projects/{id}", get(projects::get_project))
        .route("/api/projects/{id}", patch(projects::update_project))
        .route("/api/projects/{id}", delete(projects::delete_project))
        // Walk this machine's folders so the UI never asks the user to type a path.
        .route("/api/fs/browse", get(browse::browse))
        // The operating system's own folder dialog.
        .route("/api/fs/choose", post(browse::native_picker))
        .route("/api/settings", get(settings::get_settings))
        .route("/api/settings", patch(settings::patch_settings))
        .route("/api/settings/reset", post(settings::reset_settings))
        // MCP tool servers; `/registry` (the recommended list) matches ahead of `{id}`.
        .route("/api/mcp/servers", get(mcp::list_servers))
        .route("/api/mcp/servers", post(mcp::add_server))
        .route("/api/mcp/registry", get(mcp::list_registry))
        .route("/api/mcp/servers/{id}", delete(mcp::delete_server))
        .route("/api/mcp/servers/{id}/refresh", post(mcp::refresh_server))
        .route("/api/mcp/servers/{id}/enabled", post(mcp::set_enabled))
        .route("/api/mcp/servers/{id}/auth", post(mcp::set_auth))
        // Kuro's own tools, and what powers them.
        .route("/api/tools", get(tools::overview))
        .route("/api/tools/defaults", post(tools::configure_defaults))
        .route("/api/tools/skills", post(tools::set_skills))
        .route("/api/tools/skills/upload", post(tools::upload_skill))
        .route("/api/tools/skills/import", post(tools::import_skills))
        .route("/api/tools/skills/{slug}", delete(tools::delete_skill))
        .route("/api/agents", post(tools::create_agent))
        .route("/api/agents/{slug}", put(tools::update_agent))
        .route("/api/agents/{slug}", delete(tools::delete_agent))
        // Saved prompts that run on a schedule; each reply lands in its own chat.
        .route("/api/automations", get(automations::list_automations))
        .route("/api/automations", post(automations::create_automation))
        .route("/api/automations/{id}", put(automations::update_automation))
        .route("/api/automations/{id}", delete(automations::delete_automation))
        .route("/api/automations/{id}/run", post(automations::run_automation))
        .route("/api/tools/search", post(tools::configure_search))
        .route("/api/tools/search/test", post(tools::test_search))
        .route("/api/memories", get(tools::list_memories))
        .route("/api/memories", post(tools::create_memory))
        .route("/api/memories/{id}", delete(tools::delete_memory))
        // Coding workspaces: the only surface in Kuro with file access.
        .route("/api/workspaces", get(workspaces::list_workspaces))
        .route("/api/workspaces", post(workspaces::create_workspace))
        .route("/api/workspaces/{id}", get(workspaces::get_workspace))
        .route("/api/workspaces/{id}", patch(workspaces::update_workspace))
        .route("/api/workspaces/{id}", delete(workspaces::delete_workspace))
        .route("/api/workspaces/{id}/tree", get(workspaces::workspace_tree))
        .route("/api/workspaces/{id}/file", get(workspaces::read_workspace_file))
        .route("/api/workspaces/{id}/changes", get(workspaces::list_changes))
        .route(
            "/api/workspaces/{id}/changes/{change_id}/undo",
            post(workspaces::undo_change),
        )
        .route(
            "/api/workspaces/{id}/conversations",
            post(workspaces::create_conversation),
        )
        .route(
            "/api/workspaces/{id}/changes/{change_id}",
            get(workspaces::change_diff),
        )
        // Dev servers and other long-running commands, so the preview panel can show and stop them.
        .route("/api/workspaces/{id}/processes", get(workspaces::list_processes))
        .route("/api/workspaces/{id}/processes", post(workspaces::start_process))
        .route("/api/workspaces/{id}/processes", delete(workspaces::clear_processes))
        .route(
            "/api/workspaces/{id}/processes/{process_id}",
            delete(workspaces::forget_process),
        )
        .route(
            "/api/workspaces/{id}/processes/{process_id}/log",
            get(workspaces::process_log),
        )
        .route(
            "/api/workspaces/{id}/processes/{process_id}/stop",
            post(workspaces::stop_process),
        )
        // Providers with a free tier, pooled behind one model id.
        .route("/api/free", get(free::overview))
        .route("/api/free/keyless", post(free::set_keyless))
        .route("/api/free/usage", get(free::usage))
        .route("/api/usage", get(free::usage_by_model))
        .route("/api/free/{slug}/key", post(free::set_key))
        .route("/api/free/{slug}/key", delete(free::delete_key))
        .route("/api/free/{slug}/test", post(free::test_key))
        .route("/api/free/{slug}/allowance", put(free::set_allowance))
        .route("/api/free/{slug}/allowance", delete(free::delete_allowance))
        .route("/api/providers", get(providers::list_providers))
        .route("/api/providers", post(providers::add_provider))
        .route("/api/providers/{id}", delete(providers::delete_provider))
        .route("/api/providers/{id}/test", post(providers::test_provider))
        .route("/api/providers/{id}/key", post(providers::replace_key))
        .route("/api/providers/{id}/enabled", post(providers::set_enabled))
}

fn openai_routes() -> Router<SharedState> {
    Router::new()
        .route("/v1/models", get(openai::list_models))
        .route("/v1/messages", post(anthropic::messages))
        .route("/v1/chat/completions", post(openai::chat_completions))
        .route("/v1/completions", post(openai::completions))
        .route("/v1/embeddings", post(openai::embeddings))
}

#[cfg(test)]
mod tests {
    use super::is_local_request;

    #[test]
    fn the_app_the_cli_and_our_own_pages_are_let_in() {
        assert!(is_local_request(Some("127.0.0.1:8420"), None, 8420));
        assert!(is_local_request(Some("localhost:8420"), Some("http://localhost:8420"), 8420));
        assert!(is_local_request(Some("[::1]:8420"), None, 8420));
        assert!(is_local_request(None, None, 8420));
        // The Vite dev server proxies with its own origin.
        assert!(is_local_request(Some("127.0.0.1:8420"), Some("http://localhost:5173"), 8420));
    }

    #[test]
    fn another_site_or_a_rebound_name_is_refused() {
        assert!(!is_local_request(Some("127.0.0.1:8420"), Some("https://evil.example"), 8420));
        assert!(!is_local_request(Some("127.0.0.1:8420"), Some("null"), 8420));
        assert!(!is_local_request(Some("evil.example:8420"), None, 8420));
        assert!(!is_local_request(Some("localhost.evil.example"), None, 8420));
        assert!(!is_local_request(Some("127.0.0.1:8420"), Some("http://127.0.0.1:9999"), 8420));
    }
}
