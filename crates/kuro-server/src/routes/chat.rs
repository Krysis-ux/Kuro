//! The native chat endpoint.
//!
//! Unlike the OpenAI-compatible surface, which is a transparent proxy, this
//! route owns the conversation: it persists both turns, auto-titles new chats,
//! and records the usage and timing numbers the request inspector displays.

use std::convert::Infallible;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use kuro_core::db::{MessageCompletion, NewMessage};
use kuro_core::settings::Effort;
use kuro_core::sse::{drain_events, is_done};
use kuro_core::KuroError;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AppResult;
use crate::routes::common::{history_to_messages, resolve_model_id, title_from_first_line};
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(default)]
    pub model: Option<String>,
    /// `low` | `balanced` | `high` | `max`.
    #[serde(default)]
    pub effort: Option<String>,
}

/// Send a message and stream the reply.
///
/// Everything that can fail predictably — unknown conversation, unusable model,
/// an engine that will not start — is done before the stream opens, so those
/// surface as ordinary HTTP errors rather than as an error event the client has
/// to special-case.
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

    let requested_model = request
        .model
        .as_deref()
        .or(conversation.model_id.as_deref());
    let model_id = resolve_model_id(&state, requested_model).await?;

    let effort = request
        .effort
        .as_deref()
        .and_then(Effort::parse)
        .unwrap_or_default();

    // Starting the engine here means a load failure is a 503 with the engine's
    // own log tail, not a half-open stream.
    let base_url = state.engines.ensure_base_url(&model_id).await?;

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

    let history = state.db.list_messages(&conversation_id)?;
    let messages = history_to_messages(&history);

    // The assistant row exists before generation so the client has an id to
    // attach streamed text to, and so an interrupted reply is still visible.
    let assistant = state.db.insert_message(
        &conversation_id,
        &NewMessage {
            role: "assistant".to_string(),
            content: String::new(),
            model_id: Some(model_id.clone()),
            ..Default::default()
        },
    )?;

    let (sender, receiver) = mpsc::channel::<Result<Event, Infallible>>(64);

    let task_state = state.clone();
    let assistant_id = assistant.id.clone();
    let task_model_id = model_id.clone();

    tokio::spawn(async move {
        let outcome = stream_reply(
            &task_state,
            &base_url,
            &task_model_id,
            messages,
            effort,
            &assistant_id,
            &sender,
        )
        .await;

        if let Err(error) = outcome {
            tracing::warn!(model = %task_model_id, %error, "generation failed");
            let _ = task_state.db.complete_message(
                &assistant_id,
                &MessageCompletion {
                    content: String::new(),
                    finish_reason: Some("error".to_string()),
                    ..Default::default()
                },
            );
            let _ = sender
                .send(Ok(Event::default()
                    .event("error")
                    .data(json!({ "message": error.to_string() }).to_string())))
                .await;
        }
    });

    Ok(Sse::new(ReceiverStream::new(receiver))
        .keep_alive(KeepAlive::default())
        .into_response())
}

/// Drive one generation, forwarding tokens as they arrive.
#[allow(clippy::too_many_arguments)]
async fn stream_reply(
    state: &SharedState,
    base_url: &str,
    model_id: &str,
    messages: Vec<Value>,
    effort: Effort,
    assistant_id: &str,
    sender: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<(), KuroError> {
    let params = effort.params();
    let body = json!({
        "model": model_id,
        "messages": messages,
        "stream": true,
        "temperature": params.temperature,
        "top_p": params.top_p,
        "max_tokens": params.max_tokens,
        // Ask for the final usage block so token counts are the engine's own
        // numbers rather than something approximated client-side.
        "stream_options": { "include_usage": true },
    });

    let started = Instant::now();
    let response = state
        .engines
        .loopback_client()
        .post(format!("{base_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(KuroError::engine(format!(
            "the engine rejected the request ({status}): {}",
            detail.trim()
        )));
    }

    let mut content = String::new();
    let mut reasoning = String::new();
    let mut finish_reason: Option<String> = None;
    let mut prompt_tokens: Option<i64> = None;
    let mut completion_tokens: Option<i64> = None;
    let mut first_token_at: Option<Instant> = None;

    let mut buffer: Vec<u8> = Vec::new();
    let mut stream = response.bytes_stream();
    // Tracks whether the browser is still listening. When it stops — the user
    // pressed stop, closed the tab or navigated away — generation ends but
    // whatever was produced is still saved.
    let mut client_listening = true;

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
                prompt_tokens = usage.get("prompt_tokens").and_then(Value::as_i64);
                completion_tokens = usage.get("completion_tokens").and_then(Value::as_i64);
            }

            let Some(choice) = event.get("choices").and_then(|c| c.get(0)) else {
                continue;
            };

            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                finish_reason = Some(reason.to_string());
            }

            let Some(delta) = choice.get("delta") else {
                continue;
            };

            if let Some(text) = delta.get("reasoning_content").and_then(Value::as_str) {
                if !text.is_empty() {
                    reasoning.push_str(text);
                    if send_event(sender, "reasoning", json!({ "content": text })).await.is_err() {
                        client_listening = false;
                        break 'stream;
                    }
                }
            }

            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                if !text.is_empty() {
                    first_token_at.get_or_insert_with(Instant::now);
                    content.push_str(text);
                    if send_event(sender, "token", json!({ "content": text })).await.is_err() {
                        client_listening = false;
                        break 'stream;
                    }
                }
            }
        }
    }

    let total_ms = started.elapsed().as_millis() as i64;
    // Time to first token is measured here rather than taken from the engine,
    // because this is the latency the person actually waits through.
    let ttft_ms = first_token_at.map(|at| at.duration_since(started).as_millis() as i64);
    let tokens_per_second = tokens_per_second(completion_tokens, ttft_ms, total_ms);

    let resolved_finish_reason = finish_reason.clone().unwrap_or_else(|| {
        if client_listening {
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
        usage_prompt_tokens: prompt_tokens,
        // A cancelled turn gets no usage block from the engine, so fall back to
        // what was actually streamed rather than reporting nothing.
        usage_completion_tokens: completion_tokens,
        timing_ttft_ms: ttft_ms,
        timing_total_ms: Some(total_ms),
        timing_tokens_per_sec: tokens_per_second,
        finish_reason: Some(resolved_finish_reason.clone()),
    };
    state.db.complete_message(assistant_id, &completion)?;
    state.engines.touch(model_id).await;

    if !client_listening {
        tracing::debug!(
            model = model_id,
            characters = content.len(),
            "client stopped listening; partial reply saved"
        );
        return Ok(());
    }

    let _ = send_event(
        sender,
        "done",
        json!({
            "messageId": assistant_id,
            "modelId": model_id,
            "finishReason": resolved_finish_reason,
            "usage": {
                "promptTokens": prompt_tokens,
                "completionTokens": completion_tokens,
            },
            "timings": {
                "ttftMs": ttft_ms,
                "totalMs": total_ms,
                "tokensPerSecond": tokens_per_second,
            },
        }),
    )
    .await;

    Ok(())
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

/// Generation speed, excluding the time spent processing the prompt.
///
/// Dividing by wall-clock time would understate speed on a long prompt, because
/// that period produced no output tokens.
fn tokens_per_second(
    completion_tokens: Option<i64>,
    ttft_ms: Option<i64>,
    total_ms: i64,
) -> Option<f64> {
    let tokens = completion_tokens? as f64;
    let generation_ms = (total_ms - ttft_ms.unwrap_or(0)).max(1) as f64;
    if tokens <= 0.0 {
        return None;
    }
    Some(tokens / (generation_ms / 1000.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_ignores_prompt_processing_time() {
        // 100 tokens produced over one second, after a two second prompt wait.
        let rate = tokens_per_second(Some(100), Some(2000), 3000).expect("rate");
        assert!((rate - 100.0).abs() < 0.1, "got {rate}");
    }

    #[test]
    fn speed_is_absent_without_token_counts() {
        assert!(tokens_per_second(None, Some(10), 100).is_none());
        assert!(tokens_per_second(Some(0), Some(10), 100).is_none());
    }

    #[test]
    fn speed_never_divides_by_zero() {
        let rate = tokens_per_second(Some(5), Some(100), 100).expect("rate");
        assert!(rate.is_finite() && rate > 0.0);
    }
}
