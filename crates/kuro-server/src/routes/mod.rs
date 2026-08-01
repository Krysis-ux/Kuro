//! HTTP surface.
//!
//! Two API families share one server:
//!
//! * `/api/*` is Kuro's own, covering models, downloads, conversations,
//!   hardware and settings.
//! * `/v1/*` is OpenAI-compatible, so existing tools work by changing a base
//!   URL.

use axum::http::{header, HeaderValue, Method};
use axum::routing::{delete, get, patch, post, put};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

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
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Who is allowed to call this server from a browser.
///
/// This used to be `allow_origin(Any)` with a comment saying CORS existed only
/// for the Vite dev server. The comment was right about the intent and the code
/// did something much larger: it told every browser that any website in the
/// world could call this API and read the reply. Kuro has no authentication, so
/// "can call it" is the whole of the access check — and the API includes
/// `POST /api/workspaces/{id}/processes`, which runs a command. Any page you
/// visited while Kuro was running could list your workspaces and then execute
/// something in one.
///
/// So the allow-list is now what the comment always claimed: this server's own
/// origin, and the dev server. A page on any other origin gets no
/// `Access-Control-Allow-Origin` header back, and the browser refuses both the
/// preflight and the read.
///
/// ## What this does not fix
///
/// CORS governs what a *browser* will allow a page to do. It is not
/// authentication: anything that can make an HTTP request without a browser —
/// another program on this machine — is unaffected, and a page can still fire a
/// "simple" cross-origin request it cannot read the answer to. The real fix is
/// a key on every request, which changes every caller and belongs in its own
/// change. This closes the hole that is reachable from a web page today.
fn cors_layer(port: u16) -> CorsLayer {
    // Both spellings of loopback, because a browser treats them as different
    // origins and people reach the UI by whichever the address bar holds.
    let origins: Vec<HeaderValue> = [
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{DEV_SERVER_PORT}"),
        format!("http://localhost:{DEV_SERVER_PORT}"),
    ]
    .iter()
    .filter_map(|origin| origin.parse().ok())
    .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        // Everything Kuro's own client sends. `Any` here would also have let a
        // page name whatever header it liked in a preflight.
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT])
}

fn native_routes() -> Router<SharedState> {
    Router::new()
        .route("/api/health", get(system::health))
        .route("/api/status", get(system::status))
        .route("/api/hardware", get(system::hardware))
        .route("/api/shutdown", post(system::shutdown))
        .route("/api/restart", post(system::restart))
        // Literal segments are matched ahead of `{id}`, so `/models/loaded`
        // and `/models/recommended` are not captured as model ids.
        .route("/api/models", get(models::list_models))
        .route("/api/models/recommended", get(models::recommended_models))
        .route("/api/models/loaded", get(models::loaded_models))
        .route("/api/models/search", get(models::search_hub))
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
        // Editing a message truncates the conversation there and answers again;
        // forking copies it up to a message and leaves the original alone.
        .route(
            "/api/conversations/{id}/messages/{message_id}",
            patch(chat::edit_message),
        )
        .route("/api/conversations/{id}/fork", post(conversations::fork_conversation))
        .route("/api/conversations/{id}/project", post(projects::move_conversation))
        // Projects: standing instructions plus a grouping of conversations.
        .route("/api/projects", get(projects::list_projects))
        .route("/api/projects", post(projects::create_project))
        .route("/api/projects/{id}", get(projects::get_project))
        .route("/api/projects/{id}", patch(projects::update_project))
        .route("/api/projects/{id}", delete(projects::delete_project))
        // Walking this machine's folders, so nothing in the interface has to ask
        // somebody to type a path.
        .route("/api/fs/browse", get(browse::browse))
        // The operating system's own dialog, which is the one people know.
        .route("/api/fs/choose", post(browse::native_picker))
        .route("/api/settings", get(settings::get_settings))
        .route("/api/settings", patch(settings::patch_settings))
        .route("/api/settings/reset", post(settings::reset_settings))
        // MCP tool servers. `/registry` is the recommended list and is matched
        // ahead of `{id}` for the same reason `/models/loaded` is.
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
        .route("/api/tools/search", post(tools::configure_search))
        .route("/api/tools/search/test", post(tools::test_search))
        .route("/api/memories", get(tools::list_memories))
        .route("/api/memories", post(tools::create_memory))
        .route("/api/memories/{id}", delete(tools::delete_memory))
        // Coding workspaces: the only surface in Kuro with file access. Literal
        // segments come before `{id}` for the same reason `/models/loaded` does.
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
        // Dev servers and other long-running commands, so the preview panel can
        // show what is running and the user can stop it without asking a model.
        .route("/api/workspaces/{id}/processes", get(workspaces::list_processes))
        .route("/api/workspaces/{id}/processes", post(workspaces::start_process))
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
        .route("/api/free/{slug}/key", post(free::set_key))
        .route("/api/free/{slug}/key", delete(free::delete_key))
        .route("/api/free/{slug}/test", post(free::test_key))
        .route("/api/free/{slug}/allowance", put(free::set_allowance))
        .route("/api/free/{slug}/allowance", delete(free::delete_allowance))
        // Remote model providers.
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
        .route("/v1/chat/completions", post(openai::chat_completions))
        .route("/v1/completions", post(openai::completions))
        .route("/v1/embeddings", post(openai::embeddings))
}
