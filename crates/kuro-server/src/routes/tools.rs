//! Built-in tools, search configuration and memory.
//!
//! Separate from `/api/mcp` because these are Kuro's own capabilities rather than
//! someone else's server. The interface presents them together; the API keeps them
//! apart so that a user with no MCP servers at all still has a coherent tools
//! screen.

use axum::extract::{Path, Query, State};
use axum::Json;
use kuro_core::settings::{
    default_tool_groups, memory_preload_enabled, SearchSettings, KEY_SEARCH_BASE_URL,
    KEY_SEARCH_PROVIDER,
};
use kuro_core::tools::files::{FileAccess, FilePermissions};
use kuro_core::tools::web_search::{self, SearchProvider};
use kuro_core::tools::{describe_builtins, ToolGroup};
use kuro_core::skills;
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// Everything the tools screen needs in one call.
pub async fn overview(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let search = SearchSettings::resolve(&state.db)?;
    let groups: Vec<&str> = default_tool_groups(&state.db)?
        .iter()
        .map(|group| group.as_str())
        .collect();

    let enabled_skills = skills::enabled_slugs(&state.db)?;
    let active: Vec<&skills::Skill> = skills::enabled(&state.db)?;
    let files = FilePermissions::resolve(&state.db)?;

    Ok(Json(json!({
        "builtins": describe_builtins(),
        "defaultGroups": groups,
        "files": {
            "access": files.access.as_str(),
            "roots": files.root_descriptions(),
            // Whether the tools would actually be offered. A tier with no folders
            // grants nothing, and the interface says so rather than looking on.
            "usable": files.is_usable(),
            "tiers": [
                {
                    "id": FileAccess::Off.as_str(),
                    "name": "Off",
                    "description": FileAccess::Off.describe(),
                },
                {
                    "id": FileAccess::Read.as_str(),
                    "name": "Read only",
                    "description": FileAccess::Read.describe(),
                },
                {
                    "id": FileAccess::Write.as_str(),
                    "name": "Read and write",
                    "description": FileAccess::Write.describe(),
                },
            ],
        },
        "skills": {
            "catalogue": skills::SKILLS,
            "enabled": enabled_skills,
            // Shown so a user turning on six skills at once understands the cost.
            "approxTokens": skills::approx_tokens(&active),
        },
        "memory": {
            "preload": memory_preload_enabled(&state.db)?,
            "count": state.db.count_memories()?,
        },
        "search": {
            "provider": search.provider.as_str(),
            "baseUrl": search.base_url,
            // Whether a key is stored, never the key.
            "hasApiKey": state.secrets.has(SearchSettings::KEY_REFERENCE),
            "needsApiKey": search.provider.needs_api_key(),
            "needsBaseUrl": search.provider.needs_base_url(),
            "providers": [
                {
                    "id": "duckduckgo",
                    "name": "DuckDuckGo",
                    "note": "No key needed. Reads the same page a browser would, so it can \
                             break if the layout changes, and it rate-limits heavy use.",
                    "needsApiKey": false,
                    "needsBaseUrl": false,
                    "credentialsUrl": Value::Null,
                },
                {
                    "id": "brave",
                    "name": "Brave Search",
                    "note": "A proper API with a free tier. The most reliable option.",
                    "needsApiKey": true,
                    "needsBaseUrl": false,
                    "credentialsUrl": "https://brave.com/search/api/",
                },
                {
                    "id": "tavily",
                    "name": "Tavily",
                    "note": "Built for models: results come back already summarised.",
                    "needsApiKey": true,
                    "needsBaseUrl": false,
                    "credentialsUrl": "https://app.tavily.com/home",
                },
                {
                    "id": "searxng",
                    "name": "SearXNG",
                    "note": "Your own instance. Nothing is shared with a search company.",
                    "needsApiKey": false,
                    "needsBaseUrl": true,
                    "credentialsUrl": "https://docs.searxng.org/admin/installation.html",
                },
            ],
        },
    })))
}

#[derive(Debug, Deserialize)]
pub struct SearchConfigRequest {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default, rename = "baseUrl")]
    pub base_url: Option<String>,
    /// Sending an empty string clears the stored key.
    #[serde(default, rename = "apiKey")]
    pub api_key: Option<String>,
}

pub async fn configure_search(
    State(state): State<SharedState>,
    Json(request): Json<SearchConfigRequest>,
) -> AppResult<Json<Value>> {
    if let Some(provider) = &request.provider {
        let parsed = SearchProvider::parse(provider)
            .ok_or_else(|| KuroError::bad_request(format!("unknown search provider `{provider}`")))?;
        state
            .db
            .set_setting(KEY_SEARCH_PROVIDER, &json!(parsed.as_str()))?;
    }

    if let Some(base_url) = &request.base_url {
        let trimmed = base_url.trim();
        if trimmed.is_empty() {
            state.db.delete_setting(KEY_SEARCH_BASE_URL)?;
        } else {
            state
                .db
                .set_setting(KEY_SEARCH_BASE_URL, &json!(trimmed))?;
        }
    }

    if let Some(api_key) = &request.api_key {
        if api_key.trim().is_empty() {
            state.secrets.delete(SearchSettings::KEY_REFERENCE)?;
        } else {
            state.secrets.put(SearchSettings::KEY_REFERENCE, api_key)?;
        }
    }

    overview(State(state)).await
}

#[derive(Debug, Deserialize)]
pub struct SearchTestRequest {
    #[serde(default)]
    pub query: Option<String>,
}

/// Run a real search against the current configuration.
///
/// The point of this button is that a user should never have to send a message to
/// find out whether search works.
pub async fn test_search(
    State(state): State<SharedState>,
    Json(request): Json<SearchTestRequest>,
) -> AppResult<Json<Value>> {
    let query = request
        .query
        .as_deref()
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .unwrap_or("what is the model context protocol");

    let config = state.search_config()?;

    match web_search::search(&state.outbound, &config, query, 3).await {
        Ok(results) => Ok(Json(json!({
            "ok": true,
            "provider": config.provider.as_str(),
            "results": results,
        }))),
        Err(error) => Ok(Json(json!({
            "ok": false,
            "provider": config.provider.as_str(),
            "error": error.to_string(),
        }))),
    }
}

#[derive(Debug, Deserialize)]
pub struct DefaultsRequest {
    #[serde(default)]
    pub groups: Option<Vec<String>>,
    #[serde(default, rename = "memoryPreload")]
    pub memory_preload: Option<bool>,
}

pub async fn configure_defaults(
    State(state): State<SharedState>,
    Json(request): Json<DefaultsRequest>,
) -> AppResult<Json<Value>> {
    if let Some(groups) = &request.groups {
        let parsed: Vec<&str> = groups
            .iter()
            .filter_map(|name| ToolGroup::parse(name))
            .map(ToolGroup::as_str)
            .collect();
        state
            .db
            .set_setting(kuro_core::settings::KEY_DEFAULT_TOOL_GROUPS, &json!(parsed))?;
    }

    if let Some(preload) = request.memory_preload {
        state
            .db
            .set_setting(kuro_core::settings::KEY_MEMORY_PRELOAD, &json!(preload))?;
    }

    overview(State(state)).await
}

/* ---------- Files ---------- */

#[derive(Debug, Deserialize)]
pub struct FilesRequest {
    /// `off` | `read` | `write`.
    #[serde(default)]
    pub access: Option<String>,
    /// The complete list of granted folders, sent whole rather than as a diff.
    #[serde(default)]
    pub roots: Option<Vec<String>>,
}

/// Set the file access tier and the folders it applies to.
///
/// Folders are checked here rather than at call time, so that granting one that
/// does not exist is a mistake caught while the user is still looking at the
/// form. A grant that silently covers nothing is worse than a refusal: it looks
/// like access was given.
pub async fn configure_files(
    State(state): State<SharedState>,
    Json(request): Json<FilesRequest>,
) -> AppResult<Json<Value>> {
    let current = FilePermissions::resolve(&state.db)?;

    let access = match &request.access {
        Some(raw) => FileAccess::parse(raw).ok_or_else(|| {
            KuroError::bad_request(format!(
                "unknown file access level `{raw}`. Use off, read or write."
            ))
        })?,
        None => current.access,
    };

    let roots = match request.roots {
        Some(roots) => {
            for root in &roots {
                let trimmed = root.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let expanded = expand_for_check(trimmed);
                if !expanded.exists() {
                    return Err(KuroError::bad_request(format!(
                        "`{trimmed}` does not exist."
                    ))
                    .into());
                }
                if !expanded.is_dir() {
                    return Err(KuroError::bad_request(format!(
                        "`{trimmed}` is a file. Grant the folder that contains it."
                    ))
                    .into());
                }
            }
            roots
        }
        None => current.root_descriptions(),
    };

    FilePermissions::store(&state.db, access, &roots)?;
    overview(State(state)).await
}

/// Expand a leading `~` so the existence check matches what the tools will use.
fn expand_for_check(raw: &str) -> std::path::PathBuf {
    let Some(rest) = raw.strip_prefix('~') else {
        return std::path::PathBuf::from(raw);
    };
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return std::path::PathBuf::from(raw);
    };
    let rest = rest.trim_start_matches('/');
    if rest.is_empty() {
        home
    } else {
        home.join(rest)
    }
}

/* ---------- Memory ---------- */

#[derive(Debug, Deserialize)]
pub struct MemoryQuery {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

pub async fn list_memories(
    State(state): State<SharedState>,
    Query(query): Query<MemoryQuery>,
) -> AppResult<Json<Value>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let memories = match query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        Some(search) => state.db.recall_memories(search, limit)?,
        None => state.db.list_memories(limit)?,
    };

    Ok(Json(json!({ "memories": memories })))
}

#[derive(Debug, Deserialize)]
pub struct NewMemoryRequest {
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

pub async fn create_memory(
    State(state): State<SharedState>,
    Json(request): Json<NewMemoryRequest>,
) -> AppResult<Json<Value>> {
    let stored = state.db.remember(&request.content, &request.tags, None)?;
    Ok(Json(json!({ "memory": stored })))
}

pub async fn delete_memory(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let removed = state.db.forget_memory(&id)?;
    if !removed {
        return Err(KuroError::not_found(format!("memory `{id}`")).into());
    }
    Ok(Json(json!({ "deleted": true })))
}

/* ---------- Skills ---------- */

#[derive(Debug, Deserialize)]
pub struct SkillsRequest {
    pub enabled: Vec<String>,
}

/// Replace the set of active skills.
///
/// The whole set is sent rather than a diff, so the client never has to reason
/// about ordering two concurrent toggles.
pub async fn set_skills(
    State(state): State<SharedState>,
    Json(request): Json<SkillsRequest>,
) -> AppResult<Json<Value>> {
    skills::set_enabled(&state.db, &request.enabled)?;
    overview(State(state)).await
}
