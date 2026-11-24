//! Built-in tools, search configuration and memory.

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
use kuro_core::agents;
use kuro_core::db::UserAgentRecord;
use kuro_core::skills;
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::SharedState;

/// What one turn will actually carry, at each surface and effort: the ceiling on what any turn
/// spends from the pool of enabled skills.
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
            // Only real choices: the essentials are already in every coding brief, and a switch
            // would imply they are optional.
            "catalogue": skills::selectable(),
            "enabled": enabled_skills,
            // What every enabled skill would cost if all went in at once. Not the headline: a turn
            // ranks the enabled set against the request and sends only what fits `budgetTokens`.
            "approxTokens": skills::approx_tokens(&active),
            // What one turn will really carry, per surface and effort.
            "budgets": budget_summary(),
            "essentials": skills::essentials()
                .iter()
                .map(|skill| json!({
                    "slug": skill.slug,
                    "name": skill.name,
                    "blurb": skill.blurb,
                }))
                .collect::<Vec<_>>(),
            // The user's own skills, which also appear in the catalogue: the catalogue
            // answers "what can Kuro do", this one "what did I add, and from where".
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
        // The specialists a coding turn can delegate to, for the Subagents page.
        // Built-ins first, then the user's own, which carry what the editor needs.
        "agents": agents::all()
            .iter()
            .map(|agent| json!({
                "slug": agent.slug,
                "name": agent.name,
                "blurb": agent.blurb,
                "when": agent.when,
                "brief": agent.brief,
                "writes": agent.writes,
                "custom": !agents::is_builtin(agent.slug),
                "skills": agent
                    .resolved_skills()
                    .iter()
                    .map(|skill| skill.name)
                    .collect::<Vec<_>>(),
                "skillSlugs": agent.skills,
            }))
            .collect::<Vec<_>>(),
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

/// Run a real search against the current configuration, so a user need not
/// send a message to learn whether search works.
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

/// Add a skill from a `SKILL.md` the user picked. Sent as a JSON string rather
/// than multipart: it is a few kilobytes of text the client already has.
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
    // Reload before enabling: `set_enabled` drops slugs it cannot resolve, so
    // enabling first would leave the skill silently switched off.
    skills::custom::reload(&state.db)?;
    enable_new(&state, &parsed.slug)?;

    overview(State(state)).await
}

#[derive(Debug, Deserialize)]
pub struct ImportSkillsRequest {
    /// A GitHub repository, in any of the shapes people paste.
    pub url: String,
}

/// Pull every `SKILL.md` out of a GitHub repository. Built-in slugs are skipped
/// rather than failing the import, and the response says which were left.
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

    // Reload before enabling, as with an upload.
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
                // Named rather than counted, so the user can see which were skipped.
                "skipped": skipped,
                // A skill whose instructions tell the model to run `scripts/convert.py` will
                // not work, and the card does not show why.
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

/// A subagent as the editor sends it.
#[derive(Debug, Deserialize)]
pub struct AgentBody {
    name: String,
    #[serde(default)]
    blurb: String,
    #[serde(default)]
    when: String,
    brief: String,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    writes: bool,
}

impl AgentBody {
    fn into_record(self, slug: String) -> UserAgentRecord {
        UserAgentRecord {
            slug,
            name: self.name.trim().to_string(),
            blurb: self.blurb.trim().to_string(),
            when: self.when.trim().to_string(),
            brief: self.brief.trim().to_string(),
            skills: self.skills,
            writes: self.writes,
        }
    }
}

/// Add a subagent. Its slug comes from the name and must not clash with any agent.
pub async fn create_agent(
    State(state): State<SharedState>,
    Json(body): Json<AgentBody>,
) -> AppResult<Json<Value>> {
    let slug = skills::custom::slugify(&body.name);
    if agents::find(&slug).is_some() {
        return Err(KuroError::bad_request(format!(
            "There is already a subagent called `{slug}`. Pick another name."
        ))
        .into());
    }
    save_agent(&state, body.into_record(slug))?;
    overview(State(state)).await
}

/// Change one of the user's subagents. Its slug stays, even if the name changes,
/// so a conversation that delegated to it still reads the same.
pub async fn update_agent(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
    Json(body): Json<AgentBody>,
) -> AppResult<Json<Value>> {
    if agents::is_builtin(&slug) {
        return Err(KuroError::bad_request(format!(
            "`{slug}` is part of Kuro, so it cannot be changed. Make your own instead."
        ))
        .into());
    }
    if !state.db.list_user_agents()?.iter().any(|held| held.slug == slug) {
        return Err(KuroError::not_found(format!("subagent `{slug}`")).into());
    }
    save_agent(&state, body.into_record(slug))?;
    overview(State(state)).await
}

pub async fn delete_agent(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    if agents::is_builtin(&slug) {
        return Err(KuroError::bad_request(format!(
            "`{slug}` is part of Kuro, so it cannot be removed."
        ))
        .into());
    }
    if !state.db.delete_user_agent(&slug)? {
        return Err(KuroError::not_found(format!("subagent `{slug}`")).into());
    }
    agents::reload(&state.db)?;
    overview(State(state)).await
}

fn save_agent(state: &SharedState, record: UserAgentRecord) -> Result<(), KuroError> {
    agents::check(&record)?;
    state.db.put_user_agent(&record)?;
    agents::reload(&state.db)?;
    Ok(())
}

/// Switch a newly added skill on, so adding one does not need a second step.
fn enable_new(state: &SharedState, slug: &str) -> Result<(), KuroError> {
    let mut enabled = skills::enabled_slugs(&state.db)?;
    if !enabled.iter().any(|held| held == slug) {
        enabled.push(slug.to_string());
        skills::set_enabled(&state.db, &enabled)?;
    }
    Ok(())
}

/// Replace the set of active skills. The whole set is sent, not a diff, so
/// concurrent toggles need no ordering.
pub async fn set_skills(
    State(state): State<SharedState>,
    Json(request): Json<SkillsRequest>,
) -> AppResult<Json<Value>> {
    skills::set_enabled(&state.db, &request.enabled)?;
    overview(State(state)).await
}
