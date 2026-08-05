//! Built-in tools, search configuration and memory.
//!
//! Separate from `/api/mcp` because these are Kuro's own capabilities rather than
//! someone else's server. The interface presents them together; the API keeps them
//! apart so that a user with no MCP servers at all still has a coherent tools
//! screen.

use axum::extract::{Path, Query, State};
use axum::Json;
use kuro_core::orchestrate::{budget_for, Surface};
use kuro_core::settings::Effort;
use kuro_core::settings::{
    self, default_tool_groups, memory_preload_enabled, SearchSettings, KEY_SEARCH_BASE_URL,
    KEY_SEARCH_PROVIDER,
};
use kuro_core::tools::web_search::{self, SearchProvider};
use kuro_core::tools::{describe_builtins, ToolGroup};
use kuro_core::skills;
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// What one turn will actually carry, at each surface and effort.
///
/// The screen's honest headline. Switching a skill on adds it to the pool a
/// turn chooses from; these are the ceilings on what any turn spends out of
/// that pool, which is the number a user is really asking about when they look
/// at the token count and start switching things off.
fn budget_summary() -> Value {
    let describe = |surface: Surface| {
        [Effort::Low, Effort::Balanced, Effort::High, Effort::Max, Effort::Ultra]
            .iter()
            .map(|effort| json!({
                "effort": effort.as_str(),
                "tokens": budget_for(*effort, surface),
            }))
            .collect::<Vec<_>>()
    };

    json!({
        "chat": describe(Surface::Chat),
        "code": describe(Surface::Code),
    })
}

/// Everything the tools screen needs in one call.
pub async fn overview(State(state): State<SharedState>) -> AppResult<Json<Value>> {
    let search = SearchSettings::resolve(&state.db)?;
    let groups: Vec<&str> = default_tool_groups(&state.db)?
        .iter()
        .map(|group| group.as_str())
        .collect();

    let enabled_skills = skills::enabled_slugs(&state.db)?;
    let active: Vec<&skills::Skill> = skills::enabled(&state.db)?;

    Ok(Json(json!({
        "builtins": describe_builtins(),
        "defaultGroups": groups,
        "groups": ToolGroup::ALL
            .iter()
            .map(|group| json!({
                "id": group.as_str(),
                "label": group.label(),
                "blurb": group.blurb(),
            }))
            .collect::<Vec<_>>(),
        "skills": {
            // Only what is a real choice. The essentials are in every coding
            // brief already, and rendering them with a switch would say they
            // were optional.
            "catalogue": skills::selectable(),
            "enabled": enabled_skills,
            // What every enabled skill would cost if they all went in at once.
            //
            // Kept, but no longer the number the screen leads with, because it
            // stopped being what a turn spends. Skills used to be concatenated
            // into every prompt, so this total *was* the cost; a turn now ranks
            // the enabled set against what was asked and sends what fits
            // `budgetTokens`. Showing forty thousand as the running cost of
            // leaving switches on would be describing a version of Kuro that no
            // longer exists — and it is precisely the number that made people
            // switch skills off to protect a context window that was never
            // actually at risk.
            "approxTokens": skills::approx_tokens(&active),
            // What one turn will really carry, per surface and effort. The
            // honest headline.
            "budgets": budget_summary(),
            "essentials": skills::essentials()
                .iter()
                .map(|skill| json!({
                    "slug": skill.slug,
                    "name": skill.name,
                    "blurb": skill.blurb,
                }))
                .collect::<Vec<_>>(),
            // The user's own, listed separately from the catalogue they also
            // appear in. Same skills, different question: the catalogue answers
            // "what can Kuro do", and this answers "what did I add, where did
            // it come from, and how do I take it back out".
            "custom": state
                .db
                .list_user_skills()?
                .into_iter()
                .map(|held| json!({
                    "slug": held.slug,
                    "name": held.name,
                    "blurb": held.blurb,
                    "category": held.category,
                    "approxTokens": held.approx_tokens,
                    "source": held.source,
                    "updatedAt": held.updated_at,
                }))
                .collect::<Vec<_>>(),
        },
        "surfaces": {
            "chat": {
                "autoOrchestrate": settings::auto_orchestrate(&state.db, Surface::Chat)?,
                "defaultEffort": settings::default_effort(&state.db, Surface::Chat)?.as_str(),
            },
            "code": {
                "autoOrchestrate": settings::auto_orchestrate(&state.db, Surface::Code)?,
                "defaultEffort": settings::default_effort(&state.db, Surface::Code)?.as_str(),
                "defaultMode": settings::default_workspace_mode(&state.db)?.as_str(),
            },
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

#[derive(Debug, Deserialize)]
pub struct UploadSkillRequest {
    /// The file's name, used only when its contents do not say what it is.
    pub filename: String,
    pub content: String,
}

/// Add a skill from a `SKILL.md` the user picked.
///
/// The file is read here rather than uploaded as a multipart form: it is a few
/// kilobytes of text, the client already has it as a string, and a JSON body is
/// one fewer thing to get wrong in both directions.
pub async fn upload_skill(
    State(state): State<SharedState>,
    Json(request): Json<UploadSkillRequest>,
) -> AppResult<Json<Value>> {
    let parsed = skills::custom::parse_skill_md(&request.content, &request.filename)?;

    if skills::is_builtin(&parsed.slug) {
        return Err(KuroError::bad_request(format!(
            "`/{}` is already a built-in skill. Rename this one — the slug is what you type \
             after the slash, so two of them would be ambiguous.",
            parsed.slug
        ))
        .into());
    }

    let record = kuro_core::db::UserSkillRecord {
        slug: parsed.slug.clone(),
        name: parsed.name.clone(),
        blurb: parsed.blurb.clone(),
        category: parsed.category.as_str().to_string(),
        approx_tokens: skills::custom::estimate_tokens(&parsed.instructions),
        instructions: parsed.instructions.clone(),
        source: "upload".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
    };

    state.db.put_user_skill(&record)?;
    // Loaded before it is switched on, not after. `set_enabled` drops slugs it
    // cannot resolve — which is the right rule and made the wrong thing happen
    // here: the skill was stored, the enable was silently filtered out, and it
    // arrived switched off with nothing to say why.
    skills::custom::reload(&state.db)?;
    enable_new(&state, &parsed.slug)?;

    overview(State(state)).await
}

#[derive(Debug, Deserialize)]
pub struct ImportSkillsRequest {
    /// A GitHub repository, in any of the shapes people paste.
    pub url: String,
}

/// Pull every `SKILL.md` out of a GitHub repository.
///
/// Built-in slugs are skipped rather than failing the import: a repository of
/// forty skills that happens to contain one called `rust` should still deliver
/// the other thirty-nine, and the response says which were left.
pub async fn import_skills(
    State(state): State<SharedState>,
    Json(request): Json<ImportSkillsRequest>,
) -> AppResult<Json<Value>> {
    let repo = skills::import::parse_repo(&request.url)?;
    let found = skills::import::fetch_skills(&state.outbound, &repo).await?;

    let source = format!("https://github.com/{}", repo.slug());
    let mut added: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut needs_scripts: Vec<String> = Vec::new();

    for entry in &found {
        if skills::is_builtin(&entry.parsed.slug) {
            skipped.push(entry.parsed.slug.clone());
            continue;
        }
        state.db.put_user_skill(&skills::import::to_record(entry, &source))?;
        if entry.wants_scripts {
            needs_scripts.push(entry.parsed.slug.clone());
        }
        added.push(entry.parsed.slug.clone());
    }

    // Loaded before enabling, for the same reason as an upload: `set_enabled`
    // cannot keep a slug it cannot resolve.
    skills::custom::reload(&state.db)?;
    for slug in &added {
        enable_new(&state, slug)?;
    }

    let mut body = overview(State(state)).await?;
    if let Some(map) = body.0.as_object_mut() {
        map.insert(
            "imported".to_string(),
            json!({
                "repo": repo.slug(),
                "added": added,
                // Named rather than counted: "3 skipped" is a mystery, and
                // "rust, go, python were already built in" is an answer.
                "skipped": skipped,
                // Said out loud, because a skill whose instructions tell the
                // model to run `scripts/convert.py` will not work and the
                // reason is not visible from the card.
                "needsScripts": needs_scripts,
            }),
        );
    }
    Ok(body)
}

/// Remove a skill the user added. Built-ins are refused.
pub async fn delete_skill(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    if skills::is_builtin(&slug) {
        return Err(KuroError::bad_request(format!(
            "`{slug}` is part of Kuro rather than something you added, so it cannot be \
             removed. Switch it off instead."
        ))
        .into());
    }

    if !state.db.delete_user_skill(&slug)? {
        return Err(KuroError::not_found(format!("skill `{slug}`")).into());
    }

    skills::custom::reload(&state.db)?;
    overview(State(state)).await
}

/// Switch a newly added skill on.
///
/// Adding a skill and then having to go and find it in a list of sixty to turn
/// it on is a second step nobody wants and everybody forgets. Enabling it is
/// what asking for it meant.
fn enable_new(state: &SharedState, slug: &str) -> Result<(), KuroError> {
    let mut enabled = skills::enabled_slugs(&state.db)?;
    if !enabled.iter().any(|held| held == slug) {
        enabled.push(slug.to_string());
        skills::set_enabled(&state.db, &enabled)?;
    }
    Ok(())
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
