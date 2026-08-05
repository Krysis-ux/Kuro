//! Running a specialist in its own context.
//!
//! The delegating turn calls `delegate`, this runs a whole tool loop somewhere
//! else, and what comes back is the specialist's final message — a paragraph
//! rather than the forty thousand tokens of reading that produced it.
//!
//! That compression is the point. See [`kuro_core::agents`] for why a fresh
//! context is worth more here than a cleverer model would be.
//!
//! ## What a subagent is allowed
//!
//! Exactly what the workspace allows, and never more. It gets the same
//! [`Workspace`] object the parent turn has, so the mode still decides which
//! tools exist — a subagent in Plan mode cannot write any more than its parent
//! can. An agent that is not supposed to write (a reviewer, an explorer) is
//! additionally offered only the read tools, so "do not edit anything" is a
//! property of the tool set rather than a request in a prompt.

use kuro_core::agents::Agent;
use kuro_core::cloud::ChatTarget;
use kuro_core::tools::{ToolOutcome, ToolSpec};
use kuro_core::workspace::{self, CodingTool, ToolRisk, Workspace, WorkspaceContext};
use kuro_core::KuroError;
use serde_json::{json, Value};

use crate::routes::tools_runtime::{self as runtime, PartialToolCall};
use crate::state::SharedState;

/// Rounds one subagent may spend.
///
/// Lower than a top-level turn's budget on purpose. A specialist that has not
/// answered in this many rounds has been given a task too big to delegate, and
/// the right correction is for the parent to break it up rather than for this to
/// keep going.
const MAX_ROUNDS: usize = 12;

/// Output kept from the specialist's reply.
const MAX_REPORT_CHARS: usize = 12_000;

/// The `delegate` tool, as the model sees it.
pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "delegate".to_string(),
        description: format!(
            "Hand one self-contained task to a specialist, which works in its own context and \
             returns a report. Use this when a task would take several reads to answer and \
             you want the answer rather than the reading — investigating a failure, reviewing \
             what you changed, or designing something before building it. Give it everything \
             it needs in `task`: it cannot see this conversation.\n\n{}",
            kuro_core::agents::describe_for_model()
        ),
        parameters: json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "Which specialist to use.",
                    "enum": kuro_core::agents::slugs(),
                },
                "task": {
                    "type": "string",
                    "description":
                        "The whole task, written so somebody who has not read this \
                         conversation could act on it. Name the files, the symptom, and what \
                         a finished answer looks like.",
                },
            },
            "required": ["agent", "task"],
        }),
        origin: kuro_core::tools::ToolOrigin::Builtin,
    }
}

/// Run one delegated task and return the specialist's report.
pub async fn run(
    state: &SharedState,
    workspace: &Workspace,
    conversation_id: &str,
    target: &ChatTarget,
    base_url: &str,
    arguments: &Value,
) -> ToolOutcome {
    let Some(slug) = arguments.get("agent").and_then(Value::as_str) else {
        return ToolOutcome::failed("`agent` is required. Name one of the specialists listed.");
    };
    let Some(task) = arguments
        .get("task")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|task| !task.is_empty())
    else {
        return ToolOutcome::failed("`task` is required and must describe the whole task");
    };

    let Some(agent) = kuro_core::agents::find(slug) else {
        return ToolOutcome::failed(format!(
            "there is no specialist called `{slug}`. There is: {}",
            kuro_core::agents::slugs().join(", ")
        ));
    };

    match converse(state, agent, workspace, conversation_id, target, base_url, task).await {
        Ok(report) => ToolOutcome::ok(format!(
            "The {} reports:\n\n{report}",
            agent.name.to_lowercase()
        )),
        Err(error) => ToolOutcome::failed(format!("the {} could not finish: {error}", agent.name)),
    }
}

/// The specialist's own tool loop.
async fn converse(
    state: &SharedState,
    agent: &'static Agent,
    workspace: &Workspace,
    conversation_id: &str,
    target: &ChatTarget,
    base_url: &str,
    task: &str,
) -> Result<String, KuroError> {
    let specs = tools_for(agent, workspace);
    let offered: Vec<Value> = specs.iter().map(ToolSpec::to_openai).collect();

    let mut messages = vec![
        json!({ "role": "system", "content": brief(agent, workspace) }),
        json!({ "role": "user", "content": task }),
    ];

    for round in 0..=MAX_ROUNDS {
        // The last round withholds the tools, which forces an answer rather than
        // another call there is no room to service.
        let offer = (round < MAX_ROUNDS && !offered.is_empty()).then(|| json!(offered));

        let (content, calls) = complete(state, target, base_url, &messages, offer).await?;

        if calls.is_empty() {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                return Err(KuroError::other("it returned nothing"));
            }
            return Ok(workspace::exec::tail(trimmed).chars().take(MAX_REPORT_CHARS).collect());
        }

        messages.push(runtime::assistant_tool_message(&calls, &content));

        for call in &calls {
            let outcome = match specs.iter().find(|spec| spec.name == call.name) {
                Some(_) => match CodingTool::parse(&call.name) {
                    Some(tool) => {
                        let context = WorkspaceContext {
                            db: &state.db,
                            workspace,
                            conversation_id: Some(conversation_id),
                            processes: &state.processes,
                        };
                        workspace::tools::run(tool, &call.arguments, &context).await
                    }
                    // Nothing else is offered, so this is unreachable unless the
                    // offered set and this dispatch disagree.
                    None => ToolOutcome::failed(format!("`{}` is not available here", call.name)),
                },
                None => ToolOutcome::failed(format!(
                    "there is no tool called `{}`. Available: {}",
                    call.name,
                    specs
                        .iter()
                        .map(|spec| spec.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            };

            messages.push(runtime::tool_result_message(call, &outcome));
        }
    }

    Err(KuroError::other(format!(
        "it used all {MAX_ROUNDS} of its rounds without answering. The task was probably too \
         large to delegate in one piece."
    )))
}

/// The tools a specialist is offered.
///
/// The workspace mode is the ceiling and the agent's own `writes` flag lowers it
/// further. A reviewer with `edit_file` in its list will eventually use it, and
/// then the review is a diff nobody asked for.
fn tools_for(agent: &Agent, workspace: &Workspace) -> Vec<ToolSpec> {
    workspace::tools::tools_for_mode(workspace.mode)
        .into_iter()
        .filter(|tool| agent.writes || tool.risk() == ToolRisk::Read)
        .map(CodingTool::spec)
        .collect()
}

fn brief(agent: &Agent, workspace: &Workspace) -> String {
    let mut out = String::with_capacity(900);

    out.push_str(&format!("You are the {}. {}\n\n", agent.name, agent.brief));
    out.push_str(&format!(
        "You are working in the folder `{}`. Anything outside it is refused.\n",
        workspace.root.display()
    ));
    out.push_str(
        "You were given one task by another model and cannot see the conversation it came \
         from. Do not ask questions — you will get no reply. Work from what you were given, \
         and if something essential is missing, say what and answer as far as you can.\n\n\
         When you are done, reply with your findings in plain prose. That reply is the whole \
         of what gets passed back, so it must stand alone.\n\n",
    );

    let skills = agent.resolved_skills();
    if !skills.is_empty() {
        out.push_str("How to work:\n\n");
        for skill in skills {
            out.push_str(&format!("## {}\n{}\n\n", skill.name, skill.instructions.trim()));
        }
    }

    out
}

/// One non-streaming completion.
///
/// A subagent's output is not shown as it arrives — only its final report is —
/// so there is nothing to stream to and a plain request is simpler than
/// reassembling one.
async fn complete(
    state: &SharedState,
    target: &ChatTarget,
    base_url: &str,
    messages: &[Value],
    tools: Option<Value>,
) -> Result<(String, Vec<runtime::RequestedCall>), KuroError> {
    let mut body = json!({
        "model": target.wire_model(),
        "messages": messages,
        "stream": false,
        "temperature": 0.6,
        "max_tokens": 4096,
    });
    // The same treatment as `chat.rs::stream_once`. These are the only two
    // places that build a remote request, and they must not drift: a provider
    // quirk honoured in one and not the other means subagents fail on exactly
    // the providers the main loop was taught to handle.
    let quirks = target.quirks();

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

    let request = match target {
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
        return Err(KuroError::other(format!(
            "the model rejected the request ({status}): {}",
            detail.chars().take(300).collect::<String>()
        )));
    }

    let payload: Value = response.json().await?;
    let message = payload
        .pointer("/choices/0/message")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // The non-streaming shape gives whole tool calls rather than deltas, so they
    // are converted into the same struct the streaming path assembles.
    let calls: Vec<PartialToolCall> = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| PartialToolCall {
                    id: entry
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    name: entry
                        .pointer("/function/name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    arguments: entry
                        .pointer("/function/arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("{}")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok((content, runtime::read_requested_calls(&calls)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace(mode: workspace::WorkspaceMode) -> Workspace {
        Workspace {
            id: "w1".to_string(),
            root: PathBuf::from("/tmp/project"),
            mode,
        }
    }

    #[test]
    fn an_agent_that_only_reports_is_not_given_the_tools_to_edit() {
        let reviewer = kuro_core::agents::find("review").expect("reviewer");
        assert!(!reviewer.writes);

        let offered = tools_for(reviewer, &workspace(workspace::WorkspaceMode::Agent));
        let names: Vec<&str> = offered.iter().map(|spec| spec.name.as_str()).collect();

        assert!(names.contains(&"read_file"));
        assert!(
            !names.contains(&"edit_file") && !names.contains(&"write_file"),
            "a reviewer that can edit has destroyed the review; got {names:?}"
        );
        assert!(
            !names.contains(&"run_command"),
            "reporting does not require running anything"
        );
    }

    #[test]
    fn a_building_agent_gets_everything_the_mode_allows_and_no_more() {
        let backend = kuro_core::agents::find("backend").expect("backend");
        assert!(backend.writes);

        let in_agent = tools_for(backend, &workspace(workspace::WorkspaceMode::Agent));
        let names: Vec<&str> = in_agent.iter().map(|spec| spec.name.as_str()).collect();
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"run_command"));

        // The workspace mode is still the ceiling. A subagent cannot be a way
        // around the permission the user chose.
        let in_plan = tools_for(backend, &workspace(workspace::WorkspaceMode::Plan));
        let planned: Vec<&str> = in_plan.iter().map(|spec| spec.name.as_str()).collect();
        assert!(!planned.contains(&"edit_file"), "got {planned:?}");
        assert!(!planned.contains(&"run_command"));
    }

    #[test]
    fn a_subagent_is_told_it_cannot_ask_questions() {
        // It has no one to ask: its reply goes back to a model, not a person.
        let brief = brief(
            kuro_core::agents::find("explore").expect("explorer"),
            &workspace(workspace::WorkspaceMode::Plan),
        );

        assert!(brief.contains("Do not ask questions"));
        assert!(brief.contains("must stand alone"));
        assert!(brief.contains("/tmp/project"), "it must know where it is");
        assert!(brief.contains("Finding your way around"), "its skills should be loaded");
    }

    #[test]
    fn the_tool_names_every_specialist_and_says_what_each_is_for() {
        let described = spec().description;
        for agent in kuro_core::agents::AGENTS {
            assert!(described.contains(agent.slug), "`{}` missing", agent.slug);
        }
        assert!(described.contains("cannot see this conversation"));
    }
}
