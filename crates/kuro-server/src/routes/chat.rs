
use std::convert::Infallible;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use kuro_core::cloud::ChatTarget;
use kuro_core::db::{MessageCompletion, NewMessage};
use kuro_core::settings::{self, default_tool_groups, memory_preload_enabled, Effort};
use kuro_core::sse::{drain_events, is_done};
use kuro_core::tools::{fetch, intent, memory, web_search, ToolGroup, WebSource};
use kuro_core::workspace::{Workspace, WorkspaceMode};
use kuro_core::{orchestrate, prompt, skills};
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AppResult;
use crate::routes::common::{history_to_messages, resolve_target, title_from_first_line};
use crate::routes::tools_runtime::{self as runtime, PartialToolCall, ToolSet};
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub web_search: Option<bool>,
    #[serde(default)]
    pub skills: Option<Vec<String>>,
}

pub async fn send_message(
    State(state): State<SharedState>,
    Path(conversation_id): Path<String>,
    Json(request): Json<SendMessageRequest>,
) -> AppResult<Response> {
    if request.content.trim().is_empty() {
        return Err(KuroError::bad_request("message content is empty").into());
    }

    let conversation = state
        .db
        .get_conversation(&conversation_id)?
        .ok_or_else(|| KuroError::not_found(format!("conversation `{conversation_id}`")))?;

    let requested_model = request.model.as_deref().or(conversation.model_id.as_deref());
    let target = resolve_target(&state, requested_model).await?;
    let model_id = target.recorded_id();

    let effort = resolve_effort(&state, request.effort.as_deref(), &conversation)?;

    let groups = resolve_groups(&state, &request)?;
    let search_first = request.web_search.unwrap_or(false) && groups.contains(&ToolGroup::Web);

    let is_first_message = state.db.list_messages(&conversation_id)?.is_empty();
    state
        .db
        .insert_message(&conversation_id, &NewMessage::user(&request.content))?;

    if is_first_message && conversation.title_mode == "first_line" {
        state.db.set_conversation_title(
            &conversation_id,
            &title_from_first_line(&request.content),
            false,
        )?;
    }
    if conversation.model_id.as_deref() != Some(model_id.as_str()) {
        state.db.set_conversation_model(&conversation_id, &model_id)?;
    }

    let mut messages = Vec::new();

    let memory_count = state.db.count_memories()?;
    if groups.contains(&ToolGroup::Memory) && memory_preload_enabled(&state.db)? {
        let saved = state.db.list_memories(memory::MAX_PRELOADED)?;
        let about_you = settings::about_you(&state.db).unwrap_or(None);
        if let Some(preamble) = memory::preamble_with(about_you.as_deref(), &saved) {
            messages.push(json!({ "role": "system", "content": preamble }));
        }
    }

    messages.extend(history_to_messages(&state.db.list_messages(&conversation_id)?));

    let assistant = state.db.insert_message(
        &conversation_id,
        &NewMessage {
            role: "assistant".to_string(),
            content: String::new(),
            model_id: Some(model_id.clone()),
            provider_slug: target.upstream().map(str::to_string),
            ..Default::default()
        },
    )?;

    let workspace = resolve_workspace(&state, &conversation);

    let (sender, receiver) = mpsc::channel::<Result<Event, Infallible>>(64);

    let task_state = state.clone();
    let assistant_id = assistant.id.clone();
    let task_conversation_id = conversation_id.clone();

    tokio::spawn(async move {
        let outcome = run_turn(
            &task_state,
            Turn {
                conversation_id: &task_conversation_id,
                assistant_id: &assistant_id,
                target,
                effort,
                groups,
                search_first,
                memory_count,
                query: &request.content,
                pinned_skills: request.skills.as_deref().unwrap_or(&[]),
                workspace,
            },
            messages,
            &sender,
        )
        .await;

        if let Err(error) = outcome {
            tracing::warn!(%error, "generation failed");
            let _ = task_state.db.complete_message(
                &assistant_id,
                &MessageCompletion {
                    content: String::new(),
                    finish_reason: Some("error".to_string()),
                    ..Default::default()
                },
            );
            let _ = send_event(&sender, "error", json!({ "message": error.to_string() })).await;
        }
    });

    Ok(Sse::new(ReceiverStream::new(receiver))
        .keep_alive(KeepAlive::default())
        .into_response())
}

pub async fn edit_message(
    State(state): State<SharedState>,
    Path((conversation_id, message_id)): Path<(String, String)>,
    Json(request): Json<SendMessageRequest>,
) -> AppResult<Response> {
    if request.content.trim().is_empty() {
        return Err(KuroError::bad_request("message content is empty").into());
    }

    state.db.delete_from(&conversation_id, &message_id)?;

    send_message(State(state), Path(conversation_id), Json(request)).await
}

struct Turn<'a> {
    conversation_id: &'a str,
    assistant_id: &'a str,
    target: ChatTarget,
    effort: Effort,
    groups: Vec<ToolGroup>,
    search_first: bool,
    memory_count: i64,
    query: &'a str,
    pinned_skills: &'a [String],
    workspace: Option<TurnWorkspace>,
}

struct TurnWorkspace {
    workspace: Workspace,
    name: String,
    root_display: String,
}

fn resolve_effort(
    state: &SharedState,
    requested: Option<&str>,
    conversation: &kuro_core::db::Conversation,
) -> Result<Effort, KuroError> {
    if let Some(effort) = requested.and_then(Effort::parse) {
        return Ok(effort);
    }

    let surface = match conversation.workspace_id {
        Some(_) => orchestrate::Surface::Code,
        None => orchestrate::Surface::Chat,
    };

    Ok(settings::default_effort(&state.db, surface).unwrap_or_default())
}

fn resolve_workspace(state: &SharedState, conversation: &kuro_core::db::Conversation) -> Option<TurnWorkspace> {
    let workspace_id = conversation.workspace_id.as_deref()?;
    let record = state.db.get_workspace(workspace_id).ok().flatten()?;

    Some(TurnWorkspace {
        workspace: Workspace {
            id: record.id,
            root: std::path::PathBuf::from(&record.root_path),
            mode: WorkspaceMode::parse(&record.mode).unwrap_or_default(),
        },
        name: record.name,
        root_display: record.root_path,
    })
}

async fn run_turn(
    state: &SharedState,
    turn: Turn<'_>,
    mut messages: Vec<Value>,
    sender: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<(), KuroError> {
    let started = Instant::now();
    let tool_set = ToolSet::assemble(
        state,
        &turn.groups,
        turn.workspace.as_ref().map(|held| &held.workspace),
    )
    .await;
    let base_url = base_url_for(state, &turn.target).await?;

    if !tool_set.is_empty() {
        tracing::debug!(
            count = tool_set.len(),
            tools = ?tool_set.names(),
            "tools offered for this turn"
        );
    }

    let mut sources: Vec<WebSource> = Vec::new();
    let mut tool_trail: Vec<Value> = Vec::new();
    let mut used_web_search = false;

    let mut search_ran = false;
    if turn.search_first {
        match intent::decide(turn.query) {
            intent::SearchDecision::Skip(reason) => {
                tracing::debug!(reason = reason.explain(), "no up-front search for this message");
            }
            intent::SearchDecision::Search(query) => {
                used_web_search = true;
                match upfront_search(state, &query, sender).await {
                    Ok((context, found)) => {
                        search_ran = true;
                        runtime::merge_sources(&mut sources, found);
                        messages.insert(
                            messages.len().saturating_sub(1),
                            json!({ "role": "system", "content": context }),
                        );
                    }
                    Err(error) => {
                        let _ =
                            send_event(sender, "notice", json!({ "message": error.to_string() }))
                                .await;
                    }
                }
            }
        }
    }

    let tool_names: Vec<String> = tool_set.names().into_iter().map(str::to_string).collect();
    let mcp_servers = tool_set.mcp_server_names();

    let chosen_skills = skills::enabled(&state.db).unwrap_or_default();
    let surface = match &turn.workspace {
        Some(_) => orchestrate::Surface::Code,
        None => orchestrate::Surface::Chat,
    };
    let orchestration = orchestrate::plan(
        &orchestrate::Request {
            effort: turn.effort,
            surface,
            workspace: turn
                .workspace
                .as_ref()
                .map(|held| (held.workspace.root.as_path(), held.workspace.mode)),
            auto: settings::auto_orchestrate(&state.db, surface).unwrap_or(true),
            message: turn.query,
            pinned: turn.pinned_skills,
        },
        &chosen_skills,
    );
    tracing::debug!(plan = %orchestration.summary, "effort resolved");

    let active_skills = orchestration.skills.clone();

    let workspace_brief = turn.workspace.as_ref().map(|held| prompt::WorkspaceBrief {
        name: &held.name,
        root: &held.root_display,
        mode: held.workspace.mode,
    });
    let project = state
        .db
        .project_for_conversation(turn.conversation_id)
        .unwrap_or(None);
    let brief = prompt::build(&prompt::PromptContext {
        model_id: turn.target.wire_model(),
        is_remote: turn.target.is_remote(),
        web_enabled: turn.groups.contains(&ToolGroup::Web),
        search_ran,
        memory_enabled: turn.groups.contains(&ToolGroup::Memory),
        memory_count: turn.memory_count,
        tool_names: &tool_names,
        mcp_servers: &mcp_servers,
        workspace: workspace_brief,
        projects_readable: workspace_brief.is_none()
            && turn.groups.contains(&ToolGroup::Projects),
        skills: &active_skills,
        project: project.as_ref().map(|held| prompt::ProjectBrief {
            name: &held.name,
            instructions: &held.instructions,
        }),
    });
    messages.insert(0, json!({ "role": "system", "content": brief }));

    let mut content = String::new();
    let mut reasoning = String::new();
    let mut aggregate = Aggregate::default();

    let max_rounds = orchestration.max_tool_rounds;
    for round in 0..=max_rounds {
        let offer_tools = round < max_rounds;

        let step = stream_once(
            state,
            &base_url,
            &turn,
            &messages,
            offer_tools.then(|| tool_set.to_openai()).flatten(),
            sender,
        )
        .await?;

        aggregate.absorb(&step);
        if !step.content.is_empty() {
            content.push_str(&step.content);
        }
        if !step.reasoning.is_empty() {
            reasoning.push_str(&step.reasoning);
        }

        let calls = runtime::read_requested_calls(&step.tool_calls);
        if calls.is_empty() || !step.client_listening {
            aggregate.client_listening = step.client_listening;
            break;
        }

        messages.push(runtime::assistant_tool_message(&calls, &step.content));

        for call in &calls {
            let _ = send_event(
                sender,
                "tool_call",
                json!({ "name": call.name, "arguments": call.arguments }),
            )
            .await;

            let outcome = runtime::dispatch(
                state,
                &tool_set,
                turn.conversation_id,
                turn.workspace.as_ref().map(|held| &held.workspace),
                &turn.target,
                &base_url,
                call,
            )
            .await;

            if call.name == "web_search" || !outcome.sources.is_empty() {
                used_web_search = used_web_search || call.name == "web_search";
            }
            runtime::merge_sources(&mut sources, outcome.sources.clone());

            tool_trail.push(json!({
                "name": call.name,
                "arguments": call.arguments,
                "ok": !outcome.is_error,
                "preview": preview(&outcome.content),
            }));

            let _ = send_event(
                sender,
                "tool_result",
                json!({
                    "name": call.name,
                    "ok": !outcome.is_error,
                    "preview": preview(&outcome.content),
                }),
            )
            .await;

            messages.push(runtime::tool_result_message(call, &outcome));
        }
    }

    let ttft_ms = aggregate
        .first_token_at
        .map(|at| at.duration_since(started).as_millis() as i64);

    persist_and_finish(
        state,
        &turn,
        Finished {
            content,
            reasoning,
            aggregate,
            sources,
            tool_trail,
            used_web_search,
            ttft_ms,
            elapsed_ms: started.elapsed().as_millis() as i64,
        },
        sender,
    )
    .await
}

const PAGES_TO_READ: usize = 3;
const CHARS_PER_PAGE: usize = 4_000;

async fn upfront_search(
    state: &SharedState,
    query: &str,
    sender: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<(String, Vec<WebSource>), KuroError> {
    let config = state.search_config()?;

    let _ = send_event(
        sender,
        "tool_call",
        json!({ "name": "web_search", "arguments": { "query": query } }),
    )
    .await;

    let results =
        web_search::search(&state.outbound, &config, query, web_search::DEFAULT_MAX_RESULTS).await?;

    let pages = read_top_pages(state, &results).await;

    let _ = send_event(
        sender,
        "tool_result",
        json!({
            "name": "web_search",
            "ok": true,
            "preview": match pages.len() {
                0 => format!("{} results", results.len()),
                read => format!("{} results, {read} pages read", results.len()),
            },
        }),
    )
    .await;

    let mut context = String::with_capacity(4096);
    context.push_str(
        "A web search ran before you answered, so that you would not have to answer from \
         memory. Everything below came from the live web just now and is real.\n\n",
    );
    context.push_str(&web_search::format_for_model(query, &results));

    if !pages.is_empty() {
        context.push_str("\n\nThe most relevant pages, read in full:\n");
        for page in &pages {
            let title = page.title.as_deref().unwrap_or(&page.url);
            let body: String = page.text.chars().take(CHARS_PER_PAGE).collect();
            context.push_str(&format!("\n--- {title}\n{}\n{body}\n", page.url));
        }
    }

    context.push_str(
        "\n\nNow answer the user's question in your own words, using what is above. \
         Take the facts out of it and put them together — a list of links is not an answer, \
         and the interface already shows the sources under your reply. If the material does \
         not cover part of the question, answer the part it does cover and say which part it \
         did not.",
    );

    Ok((context, results.iter().map(WebSource::from).collect()))
}

async fn read_top_pages(
    state: &SharedState,
    results: &[web_search::SearchResult],
) -> Vec<fetch::FetchedPage> {
    let reads = results
        .iter()
        .take(PAGES_TO_READ)
        .map(|result| fetch::fetch_url(&state.outbound, &result.url));

    futures::future::join_all(reads)
        .await
        .into_iter()
        .filter_map(|outcome| match outcome {
            Ok(page) if !page.text.trim().is_empty() => Some(page),
            Ok(_) => None,
            Err(error) => {
                tracing::debug!(%error, "a search result could not be read");
                None
            }
        })
        .collect()
}

#[derive(Default)]
struct Step {
    content: String,
    reasoning: String,
    tool_calls: Vec<PartialToolCall>,
    finish_reason: Option<String>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    first_token_at: Option<Instant>,
    last_token_at: Option<Instant>,
    client_listening: bool,
}

impl Step {
    fn generation_ms(&self) -> i64 {
        match (self.first_token_at, self.last_token_at) {
            (Some(first), Some(last)) => last.duration_since(first).as_millis() as i64,
            _ => 0,
        }
    }
}

#[derive(Default)]
struct Aggregate {
    finish_reason: Option<String>,
    prompt_tokens: Option<i64>,
    prompt_tokens_total: Option<i64>,
    completion_tokens: Option<i64>,
    first_token_at: Option<Instant>,
    generation_ms: i64,
    client_listening: bool,
    rounds: usize,
}

impl Aggregate {
    fn absorb(&mut self, step: &Step) {
        self.rounds += 1;
        self.finish_reason = step.finish_reason.clone().or(self.finish_reason.take());
        self.prompt_tokens = step.prompt_tokens.or(self.prompt_tokens);
        self.prompt_tokens_total = match (self.prompt_tokens_total, step.prompt_tokens) {
            (Some(held), Some(new)) => Some(held + new),
            (held, new) => new.or(held),
        };
        self.completion_tokens = match (self.completion_tokens, step.completion_tokens) {
            (Some(held), Some(new)) => Some(held + new),
            (held, new) => new.or(held),
        };
        self.first_token_at = self.first_token_at.or(step.first_token_at);
        self.generation_ms += step.generation_ms();
        self.client_listening = step.client_listening;
    }
}

#[allow(clippy::too_many_arguments)]
async fn stream_once(
    state: &SharedState,
    base_url: &str,
    turn: &Turn<'_>,
    messages: &[Value],
    tools: Option<Value>,
    sender: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<Step, KuroError> {
    let params = turn.effort.params();
    let mut body = json!({
        "model": turn.target.wire_model(),
        "messages": messages,
        "stream": true,
        "temperature": params.temperature,
        "top_p": params.top_p,
        "max_tokens": params.max_tokens,
        "stream_options": { "include_usage": true },
    });

    let quirks = turn.target.quirks();

    if let Some(mut tools) = tools {
        if quirks.strip_schema_keywords {
            kuro_core::wire::strip_schema_keywords(&mut tools);
        }
        if quirks.no_parallel_tool_calls {
            body["parallel_tool_calls"] = json!(false);
        }
        body["tools"] = tools;
        body["tool_choice"] = json!("auto");
    }

    let request = match &turn.target {
        ChatTarget::Local { .. } => state
            .engines
            .loopback_client()
            .post(format!("{base_url}/v1/chat/completions")),
        ChatTarget::Remote { authorization, .. } => {
            let mut request = state
                .outbound
                .post(quirks.chat_url(base_url))
                .header("anthropic-version", "2023-06-01");

            if let Some(value) = authorization {
                request = request.header(reqwest::header::AUTHORIZATION, value);
            }
            for (name, value) in quirks.headers {
                request = request.header(*name, *value);
            }
            if let Some(timeout) = quirks.timeout {
                request = request.timeout(timeout);
            }
            request
        }
    };

    let response = request.json(&body).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(match &turn.target {
            ChatTarget::Local { .. } => KuroError::engine(format!(
                "the engine rejected the request ({status}): {}",
                detail.trim()
            )),
            ChatTarget::Remote { label, connector_id, model, .. } => {
                note_free_trouble(state, turn.target.upstream(), model, status.as_u16());
                state.providers.note_trouble(connector_id, model, status.as_u16());

                KuroError::other(format!(
                    "{label} rejected the request ({status}): {}",
                    preview(detail.trim())
                ))
            }
        });
    }

    let mut step = Step {
        client_listening: true,
        ..Default::default()
    };

    let mut buffer: Vec<u8> = Vec::new();
    let mut stream = response.bytes_stream();

    'stream: while let Some(chunk) = stream.next().await {
        buffer.extend_from_slice(&chunk?);

        for incoming in drain_events(&mut buffer) {
            if is_done(&incoming.data) {
                continue;
            }
            let Ok(event) = serde_json::from_str::<Value>(&incoming.data) else {
                continue;
            };

            if let Some(usage) = event.get("usage").filter(|value| !value.is_null()) {
                step.prompt_tokens = usage.get("prompt_tokens").and_then(Value::as_i64);
                step.completion_tokens = usage.get("completion_tokens").and_then(Value::as_i64);
            }

            let Some(choice) = event.get("choices").and_then(|c| c.get(0)) else {
                continue;
            };

            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                step.finish_reason = Some(reason.to_string());
            }

            let Some(delta) = choice.get("delta") else {
                continue;
            };

            if let Some(calls) = delta.get("tool_calls") {
                runtime::merge_tool_call_delta(&mut step.tool_calls, calls);
            }

            if let Some(text) = delta.get("reasoning_content").and_then(Value::as_str) {
                if !text.is_empty() {
                    step.reasoning.push_str(text);
                    if send_event(sender, "reasoning", json!({ "content": text }))
                        .await
                        .is_err()
                    {
                        step.client_listening = false;
                        break 'stream;
                    }
                }
            }

            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                if !text.is_empty() {
                    step.first_token_at.get_or_insert_with(Instant::now);
                    step.last_token_at = Some(Instant::now());
                    step.content.push_str(text);
                    if send_event(sender, "token", json!({ "content": text }))
                        .await
                        .is_err()
                    {
                        step.client_listening = false;
                        break 'stream;
                    }
                }
            }
        }
    }

    Ok(step)
}

fn note_free_trouble(
    state: &SharedState,
    upstream: Option<&str>,
    model: &str,
    status: u16,
) {
    let Some(trouble) = kuro_core::free::Trouble::from_status(status) else {
        return;
    };
    let Some(slug) = upstream.filter(|slug| kuro_core::free::find(slug).is_some()) else {
        return;
    };

    if trouble.stale_catalogue() {
        state.free.note_model_trouble(slug, model, trouble);
    } else {
        state.free.note_trouble(slug, trouble);
    }

    crate::routes::free::save_troubles(state);
}

struct Finished {
    content: String,
    reasoning: String,
    aggregate: Aggregate,
    sources: Vec<WebSource>,
    tool_trail: Vec<Value>,
    used_web_search: bool,
    ttft_ms: Option<i64>,
    elapsed_ms: i64,
}

async fn persist_and_finish(
    state: &SharedState,
    turn: &Turn<'_>,
    finished: Finished,
    sender: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<(), KuroError> {
    let Finished {
        content,
        reasoning,
        aggregate,
        sources,
        tool_trail,
        used_web_search,
        ttft_ms,
        elapsed_ms,
    } = finished;

    let tokens_per_second = tokens_per_second(aggregate.completion_tokens, aggregate.generation_ms);

    let resolved_finish_reason = aggregate.finish_reason.clone().unwrap_or_else(|| {
        if aggregate.client_listening {
            "stop".to_string()
        } else {
            "cancelled".to_string()
        }
    });

    let completion = MessageCompletion {
        content: content.clone(),
        reasoning_content: (!reasoning.is_empty()).then_some(reasoning),
        usage_prompt_tokens: aggregate.prompt_tokens,
        usage_prompt_tokens_total: aggregate.prompt_tokens_total,
        usage_completion_tokens: aggregate.completion_tokens,
        timing_ttft_ms: ttft_ms,
        timing_total_ms: Some(elapsed_ms),
        timing_tokens_per_sec: tokens_per_second,
        finish_reason: Some(resolved_finish_reason.clone()),
        tool_calls: (!tool_trail.is_empty()).then_some(Value::Array(tool_trail)),
        used_web_search,
        web_sources: (!sources.is_empty())
            .then(|| serde_json::to_value(&sources).ok())
            .flatten(),
    };
    state.db.complete_message(turn.assistant_id, &completion)?;

    if let ChatTarget::Local { model_id } = &turn.target {
        state.engines.touch(model_id).await;
    }

    if !aggregate.client_listening {
        tracing::debug!(
            characters = content.len(),
            "client stopped listening; partial reply saved"
        );
        return Ok(());
    }

    let _ = send_event(
        sender,
        "done",
        json!({
            "messageId": turn.assistant_id,
            "modelId": turn.target.recorded_id(),
            "finishReason": resolved_finish_reason,
            "usage": {
                "promptTokens": aggregate.prompt_tokens,
                "completionTokens": aggregate.completion_tokens,
            },
            "timings": {
                "ttftMs": ttft_ms,
                "totalMs": elapsed_ms,
                "tokensPerSecond": tokens_per_second,
            },
            "sources": sources,
            "toolRounds": aggregate.rounds.saturating_sub(1),
        }),
    )
    .await;

    Ok(())
}

async fn base_url_for(state: &SharedState, target: &ChatTarget) -> Result<String, KuroError> {
    match target {
        ChatTarget::Local { model_id } => state.engines.ensure_base_url(model_id).await,
        ChatTarget::Remote { base_url, .. } => Ok(base_url.clone()),
    }
}

fn resolve_groups(
    state: &SharedState,
    request: &SendMessageRequest,
) -> Result<Vec<ToolGroup>, KuroError> {
    match &request.tools {
        Some(names) => Ok(names
            .iter()
            .filter_map(|name| ToolGroup::parse(name))
            .collect()),
        None => default_tool_groups(&state.db),
    }
}

fn preview(text: &str) -> String {
    const LIMIT: usize = 300;
    let trimmed = text.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }
    let kept: String = trimmed.chars().take(LIMIT).collect();
    format!("{}…", kept.trim_end())
}

async fn send_event(
    sender: &mpsc::Sender<Result<Event, Infallible>>,
    name: &str,
    data: Value,
) -> Result<(), mpsc::error::SendError<Result<Event, Infallible>>> {
    sender
        .send(Ok(Event::default().event(name).data(data.to_string())))
        .await
}

fn tokens_per_second(completion_tokens: Option<i64>, generation_ms: i64) -> Option<f64> {
    let tokens = completion_tokens? as f64;
    if tokens <= 0.0 || generation_ms <= 0 {
        return None;
    }
    Some(tokens / (generation_ms as f64 / 1000.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_is_measured_over_time_spent_producing_output() {
        let rate = tokens_per_second(Some(100), 1000).expect("rate");
        assert!((rate - 100.0).abs() < 0.1, "got {rate}");
    }

    #[test]
    fn speed_is_absent_without_token_counts() {
        assert!(tokens_per_second(None, 100).is_none());
        assert!(tokens_per_second(Some(0), 100).is_none());
    }

    #[test]
    fn speed_is_absent_rather_than_absurd_when_there_is_nothing_to_measure() {
        assert!(
            tokens_per_second(Some(20), 0).is_none(),
            "one sample is not a rate"
        );
    }

    #[test]
    fn time_waiting_on_a_tool_does_not_count_as_generation_time() {
        let mut aggregate = Aggregate::default();
        let start = Instant::now();

        aggregate.absorb(&Step {
            completion_tokens: Some(12),
            client_listening: true,
            ..Default::default()
        });
        aggregate.absorb(&Step {
            completion_tokens: Some(8),
            first_token_at: Some(start),
            last_token_at: Some(start + std::time::Duration::from_millis(400)),
            client_listening: true,
            ..Default::default()
        });

        assert_eq!(aggregate.generation_ms, 400);
        let rate = tokens_per_second(aggregate.completion_tokens, aggregate.generation_ms)
            .expect("rate");
        assert!((rate - 50.0).abs() < 0.1, "20 tokens over 0.4s, got {rate}");
    }

    #[test]
    fn previews_are_bounded_and_marked() {
        assert_eq!(preview("  short  "), "short");

        let long = preview(&"x".repeat(5000));
        assert!(long.chars().count() <= 301);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn previews_do_not_split_multibyte_characters() {
        let text = "日".repeat(1000);
        let shortened = preview(&text);
        assert!(shortened.ends_with('…'));
        assert!(shortened.chars().count() <= 301);
    }

    #[test]
    fn completion_tokens_accumulate_across_tool_rounds() {
        let mut aggregate = Aggregate::default();

        aggregate.absorb(&Step {
            completion_tokens: Some(20),
            prompt_tokens: Some(100),
            client_listening: true,
            ..Default::default()
        });
        aggregate.absorb(&Step {
            completion_tokens: Some(35),
            prompt_tokens: Some(400),
            finish_reason: Some("stop".to_string()),
            client_listening: true,
            ..Default::default()
        });

        assert_eq!(
            aggregate.completion_tokens,
            Some(55),
            "output across rounds is all output the user paid for"
        );
        assert_eq!(
            aggregate.prompt_tokens,
            Some(400),
            "the last round's prompt is the one that describes what was processed"
        );
        assert_eq!(aggregate.finish_reason.as_deref(), Some("stop"));
        assert_eq!(aggregate.rounds, 2);
    }

    #[test]
    fn a_round_with_no_usage_block_does_not_erase_what_was_counted() {
        let mut aggregate = Aggregate::default();
        aggregate.absorb(&Step {
            completion_tokens: Some(20),
            client_listening: true,
            ..Default::default()
        });
        aggregate.absorb(&Step {
            client_listening: true,
            ..Default::default()
        });

        assert_eq!(aggregate.completion_tokens, Some(20));
    }

    #[test]
    fn a_client_that_stops_listening_is_remembered() {
        let mut aggregate = Aggregate::default();
        aggregate.absorb(&Step {
            client_listening: false,
            ..Default::default()
        });
        assert!(!aggregate.client_listening);
    }
}
