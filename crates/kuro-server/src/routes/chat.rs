//! The native chat endpoint.
//!
//! Unlike the OpenAI-compatible surface, which is a transparent proxy, this route
//! owns the conversation: it persists both turns, auto-titles new chats, records
//! the usage and timing numbers the request inspector displays, and runs the tool
//! loop.
//!
//! ## How tools reach a small model
//!
//! Two paths, because one is not enough in practice.
//!
//! The proper one is the OpenAI `tools` array: the model asks for a call, Kuro
//! runs it, the result goes back, repeat. That works well on models trained for
//! it and not at all on the small ones many people will actually be running —
//! a 0.5B model given a `web_search` tool will usually describe searching rather
//! than search.
//!
//! So when the user turns the web switch on explicitly, Kuro also searches *before*
//! the first token, and puts the results in front of the model as context. That is
//! deterministic: it works on every model, at every size. The tool remains
//! available for follow-up searches by models capable of asking.
//!
//! The distinction matters for honesty, too. Turning that switch on is the moment
//! a query leaves the machine, so it is a switch the user flips rather than
//! something a model decides on their behalf.

use std::convert::Infallible;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use kuro_core::cloud::ChatTarget;
use kuro_core::db::{MessageCompletion, NewMessage};
use kuro_core::settings::{default_tool_groups, memory_preload_enabled, Effort};
use kuro_core::sse::{drain_events, is_done};
use kuro_core::tools::{fetch, intent, memory, web_search, ToolGroup, WebSource};
use kuro_core::workspace::{Workspace, WorkspaceMode};
use kuro_core::{prompt, skills};
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AppResult;
use crate::routes::common::{history_to_messages, resolve_target, title_from_first_line};
use crate::routes::tools_runtime::{
    self as runtime, PartialToolCall, ToolSet, MAX_TOOL_ROUNDS,
};
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(default)]
    pub model: Option<String>,
    /// `low` | `balanced` | `high` | `max`.
    #[serde(default)]
    pub effort: Option<String>,
    /// Tool groups on for this message: `web`, `memory`. Absent means the
    /// configured default.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    /// Search before answering, regardless of whether the model would have asked.
    #[serde(default)]
    pub web_search: Option<bool>,
}

/// Send a message and stream the reply.
///
/// Everything that can fail predictably — unknown conversation, unusable model,
/// an engine that will not start — is done before the stream opens, so those
/// surface as ordinary HTTP errors rather than as an error event the client has to
/// special-case.
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

    let effort = request
        .effort
        .as_deref()
        .and_then(Effort::parse)
        .unwrap_or_default();

    let groups = resolve_groups(&state, &request)?;
    // The switch being on is what makes a search happen up front; a model may
    // still call the tool itself if the group is enabled.
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

    // Memory goes in as a system turn ahead of the history, so the model starts
    // out knowing what it has been told rather than having to ask.
    let memory_count = state.db.count_memories()?;
    if groups.contains(&ToolGroup::Memory) && memory_preload_enabled(&state.db)? {
        let saved = state.db.list_memories(memory::MAX_PRELOADED)?;
        if let Some(preamble) = memory::preamble(&saved) {
            messages.push(json!({ "role": "system", "content": preamble }));
        }
    }

    messages.extend(history_to_messages(&state.db.list_messages(&conversation_id)?));

    // The assistant row exists before generation so the client has an id to attach
    // streamed text to, and so an interrupted reply is still visible.
    let assistant = state.db.insert_message(
        &conversation_id,
        &NewMessage {
            role: "assistant".to_string(),
            content: String::new(),
            model_id: Some(model_id.clone()),
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

/// Rewrite a message and answer again from that point.
///
/// The edited message and everything after it are deleted before the new turn
/// starts. Keeping the old replies would leave a transcript where the answers
/// belong to a question that is no longer on screen — which is worse than
/// losing them, because it reads as though the model answered something it
/// never saw.
///
/// The truncation happens first and separately: if it fails, nothing has been
/// generated yet and the conversation is exactly as it was.
pub async fn edit_message(
    State(state): State<SharedState>,
    Path((conversation_id, message_id)): Path<(String, String)>,
    Json(request): Json<SendMessageRequest>,
) -> AppResult<Response> {
    if request.content.trim().is_empty() {
        return Err(KuroError::bad_request("message content is empty").into());
    }

    state.db.delete_from(&conversation_id, &message_id)?;

    // Everything after this point — persistence, titling, tools, streaming — is
    // identical to sending a new message, so it is the same code path rather
    // than a parallel one that can drift.
    send_message(State(state), Path(conversation_id), Json(request)).await
}

/// Everything about one turn that does not change between tool rounds.
struct Turn<'a> {
    conversation_id: &'a str,
    assistant_id: &'a str,
    target: ChatTarget,
    effort: Effort,
    groups: Vec<ToolGroup>,
    search_first: bool,
    /// Saved memories, so the prompt can say whether there are any.
    memory_count: i64,
    /// The user's message, used as the up-front search query.
    query: &'a str,
    /// The coding workspace this conversation belongs to. `None` for an ordinary
    /// chat, which is what makes a chat unable to reach a file.
    workspace: Option<TurnWorkspace>,
}

/// A workspace as one turn needs it: the enforcement object, plus the two
/// strings the model's brief names it by.
struct TurnWorkspace {
    workspace: Workspace,
    name: String,
    root_display: String,
}

/// Look up the workspace a conversation belongs to.
///
/// A conversation with no workspace, or one whose workspace has been deleted,
/// gets `None` — and therefore no file tools. That is the safe direction, and
/// the only one: the alternative would be a turn holding file access with
/// nothing left to scope it to.
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

/// Run one turn to completion, including any tool rounds.
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
        // Logged because "why did it not use the tool" is the first question a
        // tool problem raises, and the answer is usually that it was not offered.
        tracing::debug!(
            count = tool_set.len(),
            tools = ?tool_set.names(),
            "tools offered for this turn"
        );
    }

    let mut sources: Vec<WebSource> = Vec::new();
    let mut tool_trail: Vec<Value> = Vec::new();
    let mut used_web_search = false;

    // The deterministic search, for models that would not have asked.
    //
    // The switch being on is permission to search, not an instruction to search
    // every message. "hi" with the switch on used to return five dictionary
    // definitions of the word, which is what a model then tried to answer from.
    let mut search_ran = false;
    if turn.search_first {
        match intent::decide(turn.query) {
            intent::SearchDecision::Skip(reason) => {
                // Deliberately not a notice. Nothing went wrong, and telling
                // somebody their greeting was not searched is noise.
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
                        // A search failure is reported to the user and the turn
                        // continues without it. Refusing to answer at all would be
                        // worse — but the prompt must not then claim results are
                        // present.
                        let _ =
                            send_event(sender, "notice", json!({ "message": error.to_string() }))
                                .await;
                    }
                }
            }
        }
    }

    // The brief goes in last so it can describe what actually happened, and at
    // index 0 so it is the first thing the model reads.
    let tool_names: Vec<String> = tool_set.names().into_iter().map(str::to_string).collect();
    let mcp_servers = tool_set.mcp_server_names();
    let active_skills = skills::enabled(&state.db).unwrap_or_default();

    // The workspace, if this conversation belongs to one. Described to the model
    // exactly as it is enforced, so the brief never claims an access the tools
    // would refuse — or withholds one they would allow.
    let workspace_brief = turn.workspace.as_ref().map(|held| prompt::WorkspaceBrief {
        name: &held.name,
        root: &held.root_display,
        mode: held.workspace.mode,
    });
    // A project's standing instructions apply to every conversation in it.
    let project = state
        .db
        .project_for_conversation(turn.conversation_id)
        .unwrap_or(None);
    let brief = prompt::build(&prompt::PromptContext {
        // The name the model is known by, not Kuro's internal id — a provider
        // model's recorded id carries a connector UUID, which tells the model
        // nothing and wastes tokens.
        model_id: turn.target.wire_model(),
        is_remote: turn.target.is_remote(),
        web_enabled: turn.groups.contains(&ToolGroup::Web),
        search_ran,
        memory_enabled: turn.groups.contains(&ToolGroup::Memory),
        memory_count: turn.memory_count,
        tool_names: &tool_names,
        mcp_servers: &mcp_servers,
        workspace: workspace_brief,
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

    for round in 0..=MAX_TOOL_ROUNDS {
        // The last permitted round withholds the tools, which forces an answer
        // rather than another call the loop has no room to service.
        let offer_tools = round < MAX_TOOL_ROUNDS;

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
                // Enough to show in the inspector without storing whole pages in
                // every conversation row.
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

    // Measured against the turn's own start, so it includes any time spent in
    // tools before the first token — which is the wait the person experienced.
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

/// How many of the top results are opened and read in full.
///
/// A snippet is one or two sentences the search engine picked for matching the
/// query, which is frequently not the sentence that answers it. Handing a model
/// five snippets and asking for an answer produces a summary of a results page;
/// handing it the actual text of the top pages produces an answer. Three is the
/// most that fits alongside a conversation without crowding out the history.
const PAGES_TO_READ: usize = 3;
/// Characters kept from each page that is read.
const CHARS_PER_PAGE: usize = 4_000;

/// Search before the model has said anything, read the best results, and describe
/// what happened to the client.
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

/// Open the top few results and extract their text.
///
/// Concurrent, because three sequential page loads is most of the wait before the
/// first token. A page that refuses, times out or turns out to be a PDF is
/// dropped without comment: the snippets are still there, and one unavailable
/// source is not worth interrupting an answer for.
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

/// One request to the engine or provider, streamed.
#[derive(Default)]
struct Step {
    content: String,
    reasoning: String,
    tool_calls: Vec<PartialToolCall>,
    finish_reason: Option<String>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    first_token_at: Option<Instant>,
    /// When the last token of this round arrived, so the time actually spent
    /// producing output can be separated from time spent waiting on tools.
    last_token_at: Option<Instant>,
    client_listening: bool,
}

impl Step {
    /// Milliseconds this round spent streaming output.
    fn generation_ms(&self) -> i64 {
        match (self.first_token_at, self.last_token_at) {
            (Some(first), Some(last)) => last.duration_since(first).as_millis() as i64,
            _ => 0,
        }
    }
}

/// Numbers accumulated across every round of a turn.
#[derive(Default)]
struct Aggregate {
    finish_reason: Option<String>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    first_token_at: Option<Instant>,
    /// Summed across rounds, excluding the gaps in which tools were running.
    generation_ms: i64,
    client_listening: bool,
    rounds: usize,
}

impl Aggregate {
    fn absorb(&mut self, step: &Step) {
        self.rounds += 1;
        self.finish_reason = step.finish_reason.clone().or(self.finish_reason.take());
        // The prompt grows every round as tool results are appended, so the last
        // round's count is the one that describes what was actually processed.
        self.prompt_tokens = step.prompt_tokens.or(self.prompt_tokens);
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
        // Ask for the final usage block so token counts are the engine's own
        // numbers rather than something approximated client-side.
        "stream_options": { "include_usage": true },
    });

    if let Some(tools) = tools {
        body["tools"] = tools;
        body["tool_choice"] = json!("auto");
    }

    let request = match &turn.target {
        ChatTarget::Local { .. } => state
            .engines
            .loopback_client()
            .post(format!("{base_url}/v1/chat/completions")),
        ChatTarget::Remote { api_key, .. } => state
            .outbound
            .post(format!("{base_url}/chat/completions"))
            .bearer_auth(api_key)
            // Anthropic's compatibility endpoint wants its version header even in
            // OpenAI shape.
            .header("anthropic-version", "2023-06-01"),
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
            ChatTarget::Remote { label, .. } => KuroError::other(format!(
                "{label} rejected the request ({status}): {}",
                preview(detail.trim())
            )),
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

    // Persist unconditionally, including a reply that was cut short. Discarding
    // partial output would lose work the user already watched arrive.
    let completion = MessageCompletion {
        content: content.clone(),
        reasoning_content: (!reasoning.is_empty()).then_some(reasoning),
        usage_prompt_tokens: aggregate.prompt_tokens,
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

/// Where to send the request.
///
/// A local model needs its engine started first, which is the slow part and the
/// part that can fail; a provider needs nothing but its URL.
async fn base_url_for(state: &SharedState, target: &ChatTarget) -> Result<String, KuroError> {
    match target {
        ChatTarget::Local { model_id } => state.engines.ensure_base_url(model_id).await,
        ChatTarget::Remote { base_url, .. } => Ok(base_url.clone()),
    }
}

/// Tool groups for this message: what was asked for, else the configured default.
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

/// A short excerpt, for the inspector and for error messages.
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

/// Generation speed over the time actually spent producing output.
///
/// `generation_ms` is measured from the first streamed token to the last, summed
/// across rounds, so neither prompt processing nor time waiting on a tool is
/// counted — both produce no tokens and would understate the rate.
///
/// A span of zero means every token arrived in the same instant, which happens on
/// a very short reply and on a mocked stream. There is no honest rate to report
/// from one sample, so none is given rather than a number in the thousands.
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
        // 100 tokens streamed across one second.
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
        // A tool round can leave first and last token in the same instant. The old
        // formula divided by a clamped 1ms and reported tens of thousands of
        // tokens per second.
        assert!(
            tokens_per_second(Some(20), 0).is_none(),
            "one sample is not a rate"
        );
    }

    #[test]
    fn time_waiting_on_a_tool_does_not_count_as_generation_time() {
        let mut aggregate = Aggregate::default();
        let start = Instant::now();

        // A round that asked for a tool and streamed no text at all.
        aggregate.absorb(&Step {
            completion_tokens: Some(12),
            client_listening: true,
            ..Default::default()
        });
        // The answering round, streamed over a measurable span.
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
