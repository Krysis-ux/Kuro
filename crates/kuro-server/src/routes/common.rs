//! Helpers shared by the native and OpenAI-compatible chat routes.

use kuro_core::cloud::{self, ChatTarget};
use kuro_core::free;
use kuro_core::db::{Message, ModelStatus};
use kuro_core::settings::KEY_DEFAULT_MODEL;
use kuro_core::{KuroError, Result};
use serde_json::{json, Value};

use crate::state::SharedState;

/// Work out where a request should go.
///
/// A provider model is recognised by its `cloud:` prefix and resolved against the
/// provider registry; anything else is a local model and goes through
/// [`resolve_model_id`]. Doing this in one place means every caller — the native
/// chat route and the OpenAI-compatible one — treats local and remote alike.
pub async fn resolve_target(state: &SharedState, requested: Option<&str>) -> Result<ChatTarget> {
    let explicit = requested
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "auto" && *name != "default");

    // The free pool comes first, because a `free:` id is not a connector id and
    // would otherwise fall through to the local path and be reported missing.
    if let Some(name) = explicit.filter(|name| free::is_free_model(name)) {
        return resolve_free_target(state, name).await;
    }

    if let Some(name) = explicit.filter(|name| cloud::is_remote_model(name)) {
        return state.providers.resolve_target(name);
    }

    // A stored default may itself be a provider model, so it is checked before
    // falling through to the local-only path.
    if explicit.is_none() {
        if let Some(default) = state
            .db
            .get_setting(KEY_DEFAULT_MODEL)?
            .and_then(|value| value.as_str().map(str::to_string))
            .filter(|name| cloud::is_remote_model(name))
        {
            if let Ok(target) = state.providers.resolve_target(&default) {
                return Ok(target);
            }
        }
    }

    Ok(ChatTarget::Local {
        model_id: resolve_model_id(state, requested).await?,
    })
}

/// Turn a `free:` model id into whichever provider can actually serve it.
///
/// The failure here is worth spelling out rather than reporting as "no such
/// model": the id is real, the pool is real, and the reason nothing happens is
/// that Kuro supplies no keys and this person has not added one yet. A generic
/// not-found sends them looking for a model that was never missing.
async fn resolve_free_target(state: &SharedState, model_id: &str) -> Result<ChatTarget> {
    let selection = free::parse_selection(model_id)
        .ok_or_else(|| KuroError::not_found(format!("model `{model_id}`")))?;

    let keys = crate::routes::free::stored_keys(state);

    // A model picked by name needs no catalogue: the user already said which
    // provider and which model, so reading a list to discover that is a round
    // trip spent confirming what was passed in.
    let flavour = match selection {
        free::Selection::Flavour(flavour) => flavour,
        free::Selection::Pinned { slug, model } => {
            let choice = state.free.pinned(&slug, &model, &keys).ok_or_else(|| {
                KuroError::bad_request(format!(
                    "`{model}` needs a {slug} key, and there is not a working one yet. Add it on \
                     the Free models screen."
                ))
            })?;
            return Ok(free_target(model_id, choice));
        }
    };

    // Find out what these providers currently offer — but not while the user
    // waits. This used to be awaited here, which put the slowest of twenty
    // catalogue reads in front of the first token on every restart and again
    // every half hour. Choosing from a stale catalogue costs at worst one 404
    // and a failover; choosing fifteen seconds late costs a message that looks
    // like it never sent.
    crate::routes::free::refresh_catalogues_in_background(state, &keys);

    let Some(choice) = state.free.choose(flavour, &keys) else {
        return Err(if keys.is_empty() {
            KuroError::bad_request(
                "Kuro Free has no keys yet. It pools the free tiers of providers you sign up \
                 to — add at least one key on the Free models screen, and this model starts \
                 working.",
            )
        } else {
            KuroError::bad_request(
                "Every free provider you have added is currently out of allowance or has \
                 refused its key. Try again shortly, or add another provider.",
            )
        });
    };

    tracing::debug!(provider = %choice.slug, model = %choice.model, "free pool chose a provider");

    Ok(free_target(model_id, choice))
}

/// Turn a pool choice into somewhere to send the request.
fn free_target(model_id: &str, choice: free::Choice) -> ChatTarget {
    ChatTarget::Remote {
        // The connector id is the pool's own model id rather than a provider,
        // so the conversation records "this was Kuro Free" rather than pinning
        // it to whichever provider happened to answer that minute.
        connector_id: model_id.to_string(),
        label: format!("Kuro Free · {}", choice.name),
        base_url: choice.base_url,
        authorization: choice.authorization,
        model: choice.model,
        quirks: choice.quirks,
        // Which allowance this turn actually spends. Distinct from
        // `connector_id` above, and the line that used to be missing: without
        // it nothing downstream could say where a message went, so usage could
        // not be attributed and a failure had to guess at the provider to
        // blame.
        upstream: Some(choice.slug),
    }
}

/// Work out which local model to run.
///
/// Falls back to the configured default and then, when the machine has exactly
/// one usable model, to that one — so a first-time user who has pulled a single
/// model never has to name it.
pub async fn resolve_model_id(state: &SharedState, requested: Option<&str>) -> Result<String> {
    let explicit = requested
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "auto" && *name != "default");

    if let Some(name) = explicit {
        let model = state
            .db
            .get_model(name)?
            .ok_or_else(|| KuroError::not_found(format!("model `{name}`")))?;

        if model.status != ModelStatus::Ready {
            return Err(KuroError::bad_request(format!(
                "`{name}` is not ready to run (status: {})",
                model.status.as_str()
            )));
        }
        return Ok(model.id);
    }

    if let Some(default) = state
        .db
        .get_setting(KEY_DEFAULT_MODEL)?
        .and_then(|value| value.as_str().map(str::to_string))
    {
        if let Some(model) = state.db.get_model(&default)? {
            if model.status == ModelStatus::Ready {
                return Ok(model.id);
            }
        }
    }

    let ready: Vec<String> = state
        .db
        .list_models()?
        .into_iter()
        .filter(|model| model.status == ModelStatus::Ready)
        .map(|model| model.id)
        .collect();

    match ready.len() {
        0 => Err(KuroError::bad_request(
            "no models are installed yet. Download one — `kuro pull qwen3-4b`, or the Models \
             page — or connect a provider under Providers to use a remote one.",
        )),
        1 => Ok(ready.into_iter().next().expect("length checked")),
        _ => Err(KuroError::bad_request(format!(
            "several models are available, so one must be named: {}",
            ready.join(", ")
        ))),
    }
}

/// Convert stored history into the message array the engine expects.
///
/// Placeholder assistant rows — created before generation and still empty
/// because a request failed — are dropped so a past failure cannot corrupt the
/// context of every later turn. A row that ran tools is kept even when it has no
/// text, because what it *did* is the part later turns need.
///
/// ## Why the tool trail is replayed
///
/// This used to send `{role, content}` and nothing else, which meant a model
/// entered every turn with no record of its own actions. The failure that
/// produces is specific and bad: asked to start a dev server it starts one,
/// and asked in the next message to stop it, it answers — sincerely — that it
/// never started anything, because from inside the context window it didn't.
/// The user is then in the position of arguing with an assistant about events
/// they both witnessed, and the assistant's confidence is entirely a function
/// of having been handed an amnesiac transcript.
///
/// So each assistant turn that used tools is followed by a record of what it
/// ran. It is addressed to the model as Kuro's own log rather than as something
/// the user said, because the distinction matters when the two disagree: the log
/// is what happened, and a model that has one has no reason to reason about what
/// it "would have" done.
pub fn history_to_messages(history: &[Message]) -> Vec<Value> {
    let mut out = Vec::with_capacity(history.len());

    for message in history {
        let trail = tool_trail_of(message);

        if message.role == "assistant" && message.content.trim().is_empty() && trail.is_none() {
            continue;
        }

        out.push(json!({ "role": message.role, "content": message.content }));

        if let Some(record) = trail {
            out.push(json!({ "role": "system", "content": record }));
        }
    }

    out
}

/// The most tool calls described back to the model from any one past turn.
///
/// A long agentic turn can make dozens; replaying all of them into every
/// subsequent turn would crowd out the conversation it is supposed to be
/// supporting. The most recent ones are the ones later questions are about.
const MAX_REPLAYED_CALLS: usize = 12;
/// How much of each stored preview is replayed.
const PREVIEW_CHARS: usize = 240;

/// One assistant turn's tool calls, written out for the next turn to read.
///
/// Returns `None` when the turn used no tools, which is most of them — an
/// ordinary answer should not gain a system message describing an empty list.
fn tool_trail_of(message: &Message) -> Option<String> {
    if message.role != "assistant" {
        return None;
    }

    let calls = message.tool_calls.as_ref()?.as_array()?;
    if calls.is_empty() {
        return None;
    }

    let skipped = calls.len().saturating_sub(MAX_REPLAYED_CALLS);
    let mut record = String::with_capacity(256);
    record.push_str(
        "[Kuro's record of what you actually did in the turn above. This is the log of the \
         calls that ran, not something the user typed. If you are asked whether you did \
         something, answer from this — it is what happened.]\n",
    );

    if skipped > 0 {
        record.push_str(&format!("({skipped} earlier calls in that turn are not shown.)\n"));
    }

    for call in calls.iter().skip(skipped) {
        let name = call.get("name").and_then(Value::as_str).unwrap_or("a tool");
        let ok = call.get("ok").and_then(Value::as_bool).unwrap_or(true);
        let arguments = call
            .get("arguments")
            .map(|value| compact(&value.to_string(), PREVIEW_CHARS))
            .unwrap_or_default();
        let preview = call
            .get("preview")
            .and_then(Value::as_str)
            .map(|text| compact(text, PREVIEW_CHARS))
            .unwrap_or_default();

        record.push_str(&format!(
            "- {name}({arguments}) → {}{}\n",
            if ok { "ok" } else { "failed" },
            if preview.is_empty() {
                String::new()
            } else {
                format!(" · {preview}")
            }
        ));
    }

    Some(record)
}

/// One line, no longer than `limit` characters.
fn compact(text: &str, limit: usize) -> String {
    let single_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= limit {
        return single_line;
    }
    let kept: String = single_line.chars().take(limit).collect();
    format!("{}…", kept.trim_end())
}

/// First non-empty line of a message, trimmed to a sensible title length.
pub fn title_from_first_line(content: &str) -> String {
    const MAX_TITLE_CHARS: usize = 60;

    let first_line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("New chat");

    if first_line.chars().count() <= MAX_TITLE_CHARS {
        return first_line.to_string();
    }

    let truncated: String = first_line.chars().take(MAX_TITLE_CHARS).collect();
    format!("{}…", truncated.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, content: &str) -> Message {
        Message {
            id: "m".to_string(),
            conversation_id: "c".to_string(),
            role: role.to_string(),
            content: content.to_string(),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: None,
            used_web_search: false,
            web_sources: None,
            model_id: None,
            usage_prompt_tokens: None,
            usage_completion_tokens: None,
            timing_ttft_ms: None,
            timing_total_ms: None,
            timing_tokens_per_sec: None,
            finish_reason: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn drops_empty_assistant_placeholders_from_context() {
        let history = vec![
            message("user", "hello"),
            message("assistant", ""),
            message("user", "still there?"),
        ];

        let built = history_to_messages(&history);

        assert_eq!(built.len(), 2);
        assert_eq!(built[0]["role"], "user");
        assert_eq!(built[1]["content"], "still there?");
    }

    #[test]
    fn keeps_real_assistant_turns() {
        let history = vec![message("user", "hi"), message("assistant", "hello there")];
        assert_eq!(history_to_messages(&history).len(), 2);
    }

    fn with_tools(content: &str, calls: Value) -> Message {
        Message {
            tool_calls: Some(calls),
            ..message("assistant", content)
        }
    }

    #[test]
    fn a_turn_that_ran_tools_is_followed_by_a_record_of_them() {
        // The bug this exists for: a model that started a server, was asked to
        // stop it, and insisted it had never started one — because its own
        // actions were not in the transcript it was handed.
        let history = vec![
            message("user", "run it for me"),
            with_tools(
                "It's up at http://localhost:3000.",
                json!([{
                    "name": "start_process",
                    "arguments": { "command": "npm run dev" },
                    "ok": true,
                    "preview": "started pid 23281, serving http://localhost:3000",
                }]),
            ),
            message("user", "now stop the server"),
        ];

        let built = history_to_messages(&history);

        assert_eq!(built.len(), 4, "the record is its own message");
        let record = built[2]["content"].as_str().expect("a record");
        assert_eq!(built[2]["role"], "system");
        assert!(record.contains("start_process"), "got: {record}");
        assert!(record.contains("npm run dev"), "got: {record}");
        assert!(record.contains("23281"), "the outcome is what a follow-up asks about");
        assert!(
            record.contains("not something the user typed"),
            "the model must be able to tell its own log from a user's claim"
        );
    }

    #[test]
    fn an_answer_with_no_tools_gains_nothing() {
        let history = vec![message("user", "hi"), message("assistant", "hello")];
        let built = history_to_messages(&history);

        assert_eq!(built.len(), 2, "an ordinary turn must not grow a system message");
        assert!(built.iter().all(|entry| entry["role"] != "system"));
    }

    #[test]
    fn a_failed_call_is_recorded_as_failed() {
        let history = vec![with_tools(
            "That didn't work.",
            json!([{ "name": "read_file", "arguments": { "path": "gone.txt" }, "ok": false }]),
        )];

        let record = history_to_messages(&history)[1]["content"]
            .as_str()
            .expect("a record")
            .to_string();

        assert!(record.contains("failed"), "got: {record}");
    }

    #[test]
    fn a_turn_that_only_ran_tools_survives_the_empty_filter() {
        // Dropped before this change: the row has no text, so it looked like a
        // failed placeholder — and the actions went with it.
        let history = vec![with_tools(
            "",
            json!([{ "name": "stop_process", "arguments": {}, "ok": true }]),
        )];

        let built = history_to_messages(&history);

        assert_eq!(built.len(), 2);
        assert!(built[1]["content"].as_str().expect("record").contains("stop_process"));
    }

    #[test]
    fn a_very_long_trail_is_trimmed_rather_than_replayed_whole() {
        let calls: Vec<Value> = (0..40)
            .map(|index| json!({ "name": format!("tool_{index}"), "ok": true }))
            .collect();
        let history = vec![with_tools("done", Value::Array(calls))];

        let record = history_to_messages(&history)[1]["content"]
            .as_str()
            .expect("a record")
            .to_string();

        assert!(record.contains("earlier calls in that turn are not shown"));
        assert!(record.contains("tool_39"), "the most recent calls are the ones kept");
        assert!(!record.contains("tool_0("), "the oldest are dropped");
    }

    #[test]
    fn previews_are_flattened_and_capped_so_one_turn_cannot_dominate_the_context() {
        let history = vec![with_tools(
            "read it",
            json!([{
                "name": "read_file",
                "ok": true,
                "preview": format!("line\n{}", "x".repeat(1000)),
            }]),
        )];

        let record = history_to_messages(&history)[1]["content"]
            .as_str()
            .expect("a record")
            .to_string();

        assert!(record.chars().count() < 700, "got {} chars", record.chars().count());
        assert!(record.lines().count() <= 3, "a preview must not span lines");
    }

    #[test]
    fn title_uses_the_first_non_empty_line() {
        assert_eq!(title_from_first_line("\n\n  Explain gravity  \nmore"), "Explain gravity");
        assert_eq!(title_from_first_line(""), "New chat");
    }

    #[test]
    fn long_titles_are_truncated_with_an_ellipsis() {
        let long = "a".repeat(200);
        let title = title_from_first_line(&long);
        assert!(title.chars().count() <= 61);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn title_truncation_is_character_safe_for_non_ascii() {
        let long = "日".repeat(200);
        let title = title_from_first_line(&long);
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 61);
    }
}
