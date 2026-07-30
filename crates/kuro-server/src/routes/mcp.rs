//! MCP server management.
//!
//! Adding a server and *knowing whether it works* are the same action here: a
//! successful insert is immediately followed by a connection attempt, and the
//! result comes back in the same response. A form that saves silently and fails
//! later is the main reason MCP configuration is unpleasant elsewhere.

use axum::extract::{Path, Query, State};
use axum::Json;
use kuro_core::db::NewMcpServer;
use kuro_core::mcp::registry;
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Dial every enabled server before answering. Off by default so opening the
    /// page is instant.
    #[serde(default)]
    pub connect: bool,
}

pub async fn list_servers(
    State(state): State<SharedState>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    let servers = state.mcp.list_with_tools(query.connect).await?;
    Ok(Json(json!({ "servers": servers })))
}

/// The recommended list, marked with what is already installed.
pub async fn list_registry(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let installed: Vec<String> = state
        .db
        .list_mcp_servers()?
        .into_iter()
        .filter_map(|server| server.slug)
        .collect();

    let entries: Vec<Value> = registry::ENTRIES
        .iter()
        .map(|entry| {
            let mut encoded = serde_json::to_value(entry).unwrap_or_else(|_| json!({}));
            encoded["installed"] = json!(installed.iter().any(|slug| slug == entry.slug));
            encoded
        })
        .collect();

    Ok(Json(json!({ "entries": entries })))
}

#[derive(Debug, Deserialize)]
pub struct AddRequest {
    /// Install a recommended entry by slug. When set, the fields below are
    /// ignored apart from `authToken` and `args`.
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    pub headers: Option<serde_json::Map<String, Value>>,
    /// Bearer token. Moved straight into the credential store and never echoed
    /// back.
    #[serde(default, rename = "authToken")]
    pub auth_token: Option<String>,
}

pub async fn add_server(
    State(state): State<SharedState>,
    Json(request): Json<AddRequest>,
) -> AppResult<Json<Value>> {
    let mut input = match &request.slug {
        Some(slug) => {
            let entry = registry::find(slug)
                .ok_or_else(|| KuroError::not_found(format!("recommended server `{slug}`")))?;
            entry.to_new_server(None)
        }
        None => NewMcpServer {
            name: request.name.clone().unwrap_or_default(),
            transport: request
                .transport
                .clone()
                // A URL without a stated transport is HTTP; that is the only thing
                // a URL can mean here.
                .unwrap_or_else(|| {
                    if request.url.is_some() {
                        "http".to_string()
                    } else {
                        "stdio".to_string()
                    }
                }),
            command: request.command.clone(),
            url: request.url.clone(),
            ..Default::default()
        },
    };

    // Arguments are overridable even for a recommended entry, because the
    // filesystem server is only useful once it has been told which folder.
    if let Some(args) = request.args {
        input.args = args;
    }
    if let Some(env) = request.env {
        input.env = env;
    }
    if let Some(headers) = request.headers {
        input.headers = headers;
    }

    let token = request
        .auth_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty());

    let server = state.db.insert_mcp_server(&input, None)?;

    if let Some(token) = token {
        let reference = state.mcp.store_auth(&server.id, token)?;
        state.db.set_mcp_auth_ref(&server.id, Some(&reference))?;
    }

    // Connect now, so the response says whether it worked. A failure is reported
    // as part of the server's own record rather than as an HTTP error: the server
    // *was* added, and the user can fix its configuration.
    let connection = match state.mcp.refresh(&server.id).await {
        Ok(tools) => json!({ "ok": true, "toolCount": tools.len() }),
        Err(error) => json!({ "ok": false, "error": error.to_string() }),
    };

    let reloaded = state
        .db
        .get_mcp_server(&server.id)?
        .ok_or_else(|| KuroError::other("the server vanished after being added"))?;

    Ok(Json(json!({ "server": reloaded, "connection": connection })))
}

/// Reconnect and re-read a server's tools.
pub async fn refresh_server(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    match state.mcp.refresh(&id).await {
        Ok(tools) => Ok(Json(json!({ "ok": true, "toolCount": tools.len() }))),
        Err(error) => Ok(Json(json!({ "ok": false, "error": error.to_string() }))),
    }
}

#[derive(Debug, Deserialize)]
pub struct EnabledRequest {
    pub enabled: bool,
}

pub async fn set_enabled(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<EnabledRequest>,
) -> AppResult<Json<Value>> {
    state.db.set_mcp_enabled(&id, request.enabled)?;
    state.mcp.invalidate(&id).await;
    Ok(Json(json!({ "enabled": request.enabled })))
}

/// Replace a server's bearer token.
#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    #[serde(rename = "authToken")]
    pub auth_token: String,
}

pub async fn set_auth(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<AuthRequest>,
) -> AppResult<Json<Value>> {
    let server = state
        .db
        .get_mcp_server(&id)?
        .ok_or_else(|| KuroError::not_found(format!("MCP server `{id}`")))?;

    let reference = state.mcp.store_auth(&server.id, &request.auth_token)?;
    state.db.set_mcp_auth_ref(&server.id, Some(&reference))?;

    state.mcp.invalidate(&id).await;
    match state.mcp.refresh(&id).await {
        Ok(tools) => Ok(Json(json!({ "ok": true, "toolCount": tools.len() }))),
        Err(error) => Ok(Json(json!({ "ok": false, "error": error.to_string() }))),
    }
}

pub async fn delete_server(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let Some(server) = state.db.get_mcp_server(&id)? else {
        return Err(KuroError::not_found(format!("MCP server `{id}`")).into());
    };

    // The token goes with it. A secret outliving the thing it authenticated is a
    // liability nobody would think to clean up.
    state.mcp.forget_auth(&server)?;
    state.mcp.invalidate(&id).await;
    let removed = state.db.delete_mcp_server(&id)?;

    Ok(Json(json!({ "deleted": removed })))
}
