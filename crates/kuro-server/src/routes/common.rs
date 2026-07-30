//! Helpers shared by the native and OpenAI-compatible chat routes.

use kuro_core::cloud::{self, ChatTarget};
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
/// context of every later turn.
pub fn history_to_messages(history: &[Message]) -> Vec<Value> {
    history
        .iter()
        .filter(|message| !(message.role == "assistant" && message.content.trim().is_empty()))
        .map(|message| json!({ "role": message.role, "content": message.content }))
        .collect()
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
