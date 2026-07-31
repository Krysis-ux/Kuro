//! Assembling and running the tools available to one turn.
//!
//! Kept out of `chat.rs` so that route reads as "stream a reply" rather than as
//! "stream a reply and also manage a tool registry". Everything here is about
//! producing a [`ToolSet`] and then dispatching against it.

use kuro_core::tools::{
    self, Builtin, BuiltinContext, ToolGroup, ToolOrigin, ToolOutcome, ToolSpec, WebSource,
};
use kuro_core::workspace::{self, CodingTool, Workspace, WorkspaceContext};
use serde_json::Value;

use crate::state::SharedState;

/// Ceiling on tool rounds in one turn.
///
/// A model that has not finished after this many rounds is usually looping — the
/// classic case being a search that returns nothing and gets retried verbatim.
/// Stopping and answering with what it has is better than spending the user's
/// afternoon on it.
pub const MAX_TOOL_ROUNDS: usize = 6;

/// The tools offered for one turn, and where each came from.
pub struct ToolSet {
    specs: Vec<ToolSpec>,
}

impl ToolSet {
    /// Build the set from the groups the user enabled, the workspace this turn
    /// is running in if there is one, and every connected MCP server.
    ///
    /// Built-ins are added first so that an MCP server cannot take the name
    /// `web_search` and quietly replace it. Workspace tools come next, for the
    /// same reason: a server offering `edit_file` must not be able to become the
    /// thing the model reaches for when it wants to change the user's code.
    pub async fn assemble(
        state: &SharedState,
        groups: &[ToolGroup],
        workspace: Option<&Workspace>,
    ) -> Self {
        let mut specs: Vec<ToolSpec> = tools::builtins_for_groups(groups)
            .into_iter()
            .map(Builtin::spec)
            .collect();

        // Only a workspace grants file access, and only up to what its mode
        // allows. An ordinary chat passes `None` and gets nothing.
        if let Some(workspace) = workspace {
            specs.extend(
                workspace::tools::tools_for_mode(workspace.mode)
                    .into_iter()
                    .map(CodingTool::spec),
            );
        }

        let mut taken: Vec<String> = specs.iter().map(|spec| spec.name.clone()).collect();

        match state.mcp.tool_specs(&mut taken).await {
            Ok(mcp_specs) => specs.extend(mcp_specs),
            Err(error) => {
                // A broken MCP configuration must not stop a conversation that
                // was not relying on it.
                tracing::warn!(%error, "could not collect MCP tools for this turn");
            }
        }

        Self { specs }
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// The `tools` array for the request body, or `None` when there is nothing to
    /// offer. An empty array is not the same as an absent field: some engines
    /// treat `"tools": []` as a reason to refuse the request.
    pub fn to_openai(&self) -> Option<Value> {
        if self.specs.is_empty() {
            return None;
        }
        Some(Value::Array(
            self.specs.iter().map(ToolSpec::to_openai).collect(),
        ))
    }

    pub fn find(&self, name: &str) -> Option<&ToolSpec> {
        self.specs.iter().find(|spec| spec.name == name)
    }

    /// Names offered, for the log line that explains what a turn had available.
    pub fn names(&self) -> Vec<&str> {
        self.specs.iter().map(|spec| spec.name.as_str()).collect()
    }

    /// Distinct MCP servers behind the tools in this set, in the order first seen.
    ///
    /// Free, because the origins are already here. The model's brief names these
    /// so that "what did I just connect" has an answer that does not require
    /// guessing which server a tool name belongs to.
    pub fn mcp_server_names(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for spec in &self.specs {
            if let ToolOrigin::Mcp { server_name, .. } = &spec.origin {
                if !seen.iter().any(|held| held == server_name) {
                    seen.push(server_name.clone());
                }
            }
        }
        seen
    }
}

/// One tool call the model asked for.
#[derive(Debug, Clone)]
pub struct RequestedCall {
    /// Correlation id from the engine, echoed back on the result message.
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Run one call and describe what happened.
pub async fn dispatch(
    state: &SharedState,
    set: &ToolSet,
    conversation_id: &str,
    workspace: Option<&Workspace>,
    call: &RequestedCall,
) -> ToolOutcome {
    let Some(spec) = set.find(&call.name) else {
        // The model invented a tool. Telling it so is more useful than failing the
        // turn, because it usually then picks a real one.
        return ToolOutcome::failed(format!(
            "there is no tool called `{}`. Available: {}",
            call.name,
            set.names().join(", ")
        ));
    };

    match &spec.origin {
        ToolOrigin::Builtin => {
            // A workspace tool and a chat built-in are both `Builtin` origin, so
            // the coding set is checked first — and only reachable at all when
            // this turn actually has a workspace.
            if let Some(tool) = CodingTool::parse(&spec.name) {
                let Some(workspace) = workspace else {
                    return ToolOutcome::failed(format!(
                        "`{}` only works inside a coding workspace",
                        spec.name
                    ));
                };
                let context = WorkspaceContext {
                    db: &state.db,
                    workspace,
                    conversation_id: Some(conversation_id),
                };
                return workspace::tools::run(tool, &call.arguments, &context);
            }

            let Some(builtin) = Builtin::parse(&spec.name) else {
                return ToolOutcome::failed(format!("`{}` is not wired up", spec.name));
            };

            let search = match state.search_config() {
                Ok(config) => config,
                Err(error) => return ToolOutcome::failed(error),
            };

            let context = BuiltinContext {
                db: &state.db,
                client: &state.outbound,
                search,
                conversation_id: Some(conversation_id),
            };
            tools::run_builtin(builtin, &call.arguments, &context).await
        }
        ToolOrigin::Mcp {
            server_id,
            server_name,
            remote_name,
        } => match state.mcp.call(server_id, remote_name, &call.arguments).await {
            Ok((content, is_error)) => ToolOutcome {
                content,
                is_error,
                sources: Vec::new(),
            },
            Err(error) => ToolOutcome::failed(format!("{server_name}: {error}")),
        },
    }
}

/// Read the tool calls out of a completed assistant message.
///
/// Streaming assembles `tool_calls` across deltas, so by the time this is called
/// the fragments have already been joined; this only validates them.
pub fn read_requested_calls(raw: &[PartialToolCall]) -> Vec<RequestedCall> {
    raw.iter()
        .filter(|call| !call.name.trim().is_empty())
        .enumerate()
        .map(|(index, call)| RequestedCall {
            // A model occasionally omits the id. Substituting a positional one
            // keeps the result message pairable, which matters because an
            // unpaired tool result makes the next request invalid.
            id: if call.id.trim().is_empty() {
                format!("call_{index}")
            } else {
                call.id.clone()
            },
            name: call.name.trim().to_string(),
            arguments: tools::parse_arguments(Some(&Value::String(call.arguments.clone()))),
        })
        .collect()
}

/// A tool call being assembled from streaming deltas.
#[derive(Debug, Clone, Default)]
pub struct PartialToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON text, accumulated across deltas.
    pub arguments: String,
}

/// Merge one streamed `tool_calls` delta into the calls assembled so far.
///
/// The engine sends a tool call in pieces: an index, then the name, then the
/// argument JSON a few characters at a time. Indexes are not necessarily
/// contiguous or in order, so the vector is grown to fit rather than pushed to.
pub fn merge_tool_call_delta(calls: &mut Vec<PartialToolCall>, delta: &Value) {
    let Some(entries) = delta.as_array() else { return };

    for entry in entries {
        let index = entry.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        if index >= calls.len() {
            calls.resize(index + 1, PartialToolCall::default());
        }
        let held = &mut calls[index];

        if let Some(id) = entry.get("id").and_then(Value::as_str) {
            if !id.is_empty() {
                held.id = id.to_string();
            }
        }
        if let Some(name) = entry.pointer("/function/name").and_then(Value::as_str) {
            if !name.is_empty() {
                held.name.push_str(name);
            }
        }
        if let Some(arguments) = entry.pointer("/function/arguments").and_then(Value::as_str) {
            held.arguments.push_str(arguments);
        }
    }
}

/// The assistant message to store and replay, carrying the calls it made.
pub fn assistant_tool_message(calls: &[RequestedCall], content: &str) -> Value {
    serde_json::json!({
        "role": "assistant",
        "content": content,
        "tool_calls": calls
            .iter()
            .map(|call| serde_json::json!({
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    // Re-serialised rather than passed through, so a model that
                    // sent malformed JSON does not send it again on the next round.
                    "arguments": call.arguments.to_string(),
                },
            }))
            .collect::<Vec<_>>(),
    })
}

/// The result message paired to a call.
pub fn tool_result_message(call: &RequestedCall, outcome: &ToolOutcome) -> Value {
    serde_json::json!({
        "role": "tool",
        "tool_call_id": call.id,
        "name": call.name,
        "content": outcome.content,
    })
}

/// Merge new sources into those already collected, keeping the first mention of
/// each URL and dropping repeats.
pub fn merge_sources(collected: &mut Vec<WebSource>, incoming: Vec<WebSource>) {
    for source in incoming {
        if !collected.iter().any(|held| held.url == source.url) {
            collected.push(source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn assembles_a_tool_call_from_streamed_fragments() {
        let mut calls: Vec<PartialToolCall> = Vec::new();

        merge_tool_call_delta(
            &mut calls,
            &json!([{ "index": 0, "id": "call_abc", "function": { "name": "web_search" } }]),
        );
        merge_tool_call_delta(
            &mut calls,
            &json!([{ "index": 0, "function": { "arguments": "{\"query\":" } }]),
        );
        merge_tool_call_delta(
            &mut calls,
            &json!([{ "index": 0, "function": { "arguments": "\"rust\"}" } }]),
        );

        let requested = read_requested_calls(&calls);

        assert_eq!(requested.len(), 1);
        assert_eq!(requested[0].id, "call_abc");
        assert_eq!(requested[0].name, "web_search");
        assert_eq!(requested[0].arguments["query"], "rust");
    }

    #[test]
    fn a_name_split_across_deltas_is_joined() {
        let mut calls: Vec<PartialToolCall> = Vec::new();
        merge_tool_call_delta(&mut calls, &json!([{ "index": 0, "function": { "name": "web_" } }]));
        merge_tool_call_delta(&mut calls, &json!([{ "index": 0, "function": { "name": "search" } }]));

        assert_eq!(read_requested_calls(&calls)[0].name, "web_search");
    }

    #[test]
    fn parallel_calls_are_kept_apart_even_out_of_order() {
        let mut calls: Vec<PartialToolCall> = Vec::new();

        merge_tool_call_delta(
            &mut calls,
            &json!([{ "index": 1, "id": "b", "function": { "name": "recall", "arguments": "{}" } }]),
        );
        merge_tool_call_delta(
            &mut calls,
            &json!([{ "index": 0, "id": "a", "function": { "name": "web_search", "arguments": "{\"query\":\"x\"}" } }]),
        );

        let requested = read_requested_calls(&calls);

        assert_eq!(requested.len(), 2);
        assert_eq!(requested[0].name, "web_search");
        assert_eq!(requested[1].name, "recall");
    }

    #[test]
    fn a_call_with_no_id_still_gets_a_pairable_one() {
        let calls = vec![PartialToolCall {
            id: String::new(),
            name: "recall".to_string(),
            arguments: "{}".to_string(),
        }];

        let requested = read_requested_calls(&calls);

        assert_eq!(requested[0].id, "call_0");
        assert!(!requested[0].id.is_empty(), "an unpaired result invalidates the next request");
    }

    #[test]
    fn a_nameless_fragment_is_discarded_rather_than_called() {
        let calls = vec![PartialToolCall::default()];
        assert!(read_requested_calls(&calls).is_empty());
    }

    #[test]
    fn malformed_arguments_become_an_empty_object() {
        let calls = vec![PartialToolCall {
            id: "a".to_string(),
            name: "web_search".to_string(),
            arguments: "{not json".to_string(),
        }];

        assert_eq!(read_requested_calls(&calls)[0].arguments, json!({}));
    }

    #[test]
    fn a_delta_that_is_not_an_array_is_ignored() {
        let mut calls: Vec<PartialToolCall> = Vec::new();
        merge_tool_call_delta(&mut calls, &json!({ "index": 0 }));
        merge_tool_call_delta(&mut calls, &Value::Null);
        assert!(calls.is_empty());
    }

    #[test]
    fn the_assistant_message_re_serialises_arguments() {
        let calls = vec![RequestedCall {
            id: "a".to_string(),
            name: "web_search".to_string(),
            arguments: json!({ "query": "rust" }),
        }];

        let message = assistant_tool_message(&calls, "");

        assert_eq!(message["role"], "assistant");
        assert_eq!(message["tool_calls"][0]["function"]["name"], "web_search");
        let arguments = message["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .expect("arguments must be a string");
        assert_eq!(
            serde_json::from_str::<Value>(arguments).expect("valid json")["query"],
            "rust"
        );
    }

    #[test]
    fn a_result_message_is_paired_to_its_call() {
        let call = RequestedCall {
            id: "call_1".to_string(),
            name: "recall".to_string(),
            arguments: json!({}),
        };

        let message = tool_result_message(&call, &ToolOutcome::ok("nothing yet"));

        assert_eq!(message["role"], "tool");
        assert_eq!(message["tool_call_id"], "call_1");
        assert_eq!(message["content"], "nothing yet");
    }

    #[test]
    fn an_empty_tool_set_sends_no_tools_field_at_all() {
        let set = ToolSet { specs: Vec::new() };
        assert!(set.is_empty());
        assert!(
            set.to_openai().is_none(),
            "an empty array makes some engines refuse the request"
        );
    }

    #[test]
    fn a_populated_set_encodes_every_tool() {
        let set = ToolSet {
            specs: vec![Builtin::WebSearch.spec(), Builtin::Recall.spec()],
        };

        let encoded = set.to_openai().expect("tools");

        assert_eq!(encoded.as_array().expect("array").len(), 2);
        assert_eq!(set.len(), 2);
        assert!(set.find("web_search").is_some());
        assert!(set.find("nonexistent").is_none());
    }

    #[test]
    fn sources_are_deduplicated_by_url_keeping_the_first_title() {
        let mut collected = vec![WebSource {
            title: "First".to_string(),
            url: "https://a.example".to_string(),
        }];

        merge_sources(
            &mut collected,
            vec![
                WebSource {
                    title: "Renamed".to_string(),
                    url: "https://a.example".to_string(),
                },
                WebSource {
                    title: "Second".to_string(),
                    url: "https://b.example".to_string(),
                },
            ],
        );

        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].title, "First");
        assert_eq!(collected[1].url, "https://b.example");
    }
}
