//! `POST /v1/messages` — the Anthropic-compatible endpoint.
//!
//! This is what makes `kuro launch claude` possible. Claude Code speaks the
//! Messages API and nothing else; every endpoint Kuro can reach speaks the
//! OpenAI shape. Point Claude Code here and it talks to a local llama.cpp, an
//! OpenRouter key or a free provider without knowing that any of that happened.
//!
//! The translation itself lives in [`kuro_core::gateway`], with its own tests.
//! What is here is the part that needs the server: resolving which endpoint the
//! request should go to, and turning one stream into the other.
//!
//! ## Kuro's own tools are deliberately absent
//!
//! Claude Code brings its own — it edits files, runs commands and asks for
//! permission in its own interface, under its own rules. Kuro's contribution on
//! this path is the model. Offering Kuro's coding tools here as well would mean
//! two tool layers with two different ideas of what is permitted, and the
//! workspace mode that governs Kuro's own surfaces would not be governing
//! anything.

use axum::body::Body;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::convert::Infallible;

use futures::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use kuro_core::cloud::ChatTarget;
use kuro_core::gateway::{self, StreamTranslator};
use kuro_core::sse::drain_events;
use serde_json::{json, Value};

use crate::routes::common::resolve_target;
use crate::state::SharedState;

// `/v1/models` is deliberately not reimplemented here. The OpenAI-shaped one
// already answers with a `data` array of `{id}`, which is what a client reads
// to validate a model name, and two handlers on one path would mean whichever
// was registered last silently won.
pub async fn messages(State(state): State<SharedState>, Json(body): Json<Value>) -> Response {
    // Whatever Claude Code was configured with. An unknown name is not an
    // error: `resolve_target` falls back to Kuro's own default, which is what
    // makes `kuro launch claude` work without teaching Claude Code any of
    // Kuro's model ids.
    let requested = gateway::requested_model(&body);

    let target = match resolve_target(&state, requested).await {
        Ok(target) => target,
        Err(error) => return failed(StatusCode::BAD_REQUEST, &error.to_string()),
    };

    // A local model has to be running before anything is sent to it, and the
    // address it is listening on is only known once it is.
    let base_url = match &target {
        ChatTarget::Local { model_id } => match state.engines.ensure_base_url(model_id).await {
            Ok(url) => url,
            Err(error) => return failed(StatusCode::SERVICE_UNAVAILABLE, &error.to_string()),
        },
        ChatTarget::Remote { base_url, .. } => base_url.clone(),
    };

    let wire_model = target.wire_model().to_string();
    let mut upstream = gateway::to_openai_request(&body, &wire_model);

    let quirks = target.quirks();
    if let Some(tools) = upstream.get_mut("tools") {
        if quirks.strip_schema_keywords {
            kuro_core::wire::strip_schema_keywords(tools);
        }
    }
    if quirks.no_parallel_tool_calls && upstream.get("tools").is_some() {
        upstream["parallel_tool_calls"] = json!(false);
    }

    if gateway::wants_stream(&body) {
        stream(state, target, base_url, upstream, wire_model).await
    } else {
        once(state, target, base_url, upstream, wire_model).await
    }
}

/// Build the upstream request for either kind of target.
///
/// The third place in this codebase that does this, and the comment in
/// `subagent.rs` warns about exactly that: a provider quirk honoured in one and
/// not the others means requests fail on precisely the providers the main loop
/// was taught to handle. Kept identical on purpose.
fn upstream_request(
    state: &SharedState,
    target: &ChatTarget,
    base_url: &str,
    body: &Value,
) -> reqwest::RequestBuilder {
    let quirks = target.quirks();

    match target {
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
    }
    .json(body)
}

/// One request, one JSON answer.
async fn once(
    state: SharedState,
    target: ChatTarget,
    base_url: String,
    upstream: Value,
    model: String,
) -> Response {
    let response = match upstream_request(&state, &target, &base_url, &upstream).send().await {
        Ok(response) => response,
        Err(error) => return failed(StatusCode::BAD_GATEWAY, &error.to_string()),
    };

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return failed(status, &first_line(&detail));
    }

    match response.json::<Value>().await {
        Ok(payload) => Json(gateway::to_anthropic_response(&payload, &model)).into_response(),
        Err(error) => failed(StatusCode::BAD_GATEWAY, &error.to_string()),
    }
}

/// One request, a translated event stream.
async fn stream(
    state: SharedState,
    target: ChatTarget,
    base_url: String,
    upstream: Value,
    model: String,
) -> Response {
    let response = match upstream_request(&state, &target, &base_url, &upstream).send().await {
        Ok(response) => response,
        Err(error) => return failed(StatusCode::BAD_GATEWAY, &error.to_string()),
    };

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return failed(status, &first_line(&detail));
    }

    // A channel rather than a generator: `async-stream` is not a dependency
    // here, and the chat route already streams this way.
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<String, Infallible>>(64);

    tokio::spawn(async move {
        let mut translator = StreamTranslator::new(model);
        let mut buffer: Vec<u8> = Vec::new();
        let mut source = response.bytes_stream();

        while let Some(chunk) = source.next().await {
            let bytes = match chunk {
                Ok(bytes) => bytes,
                Err(_) => {
                    // The connection died mid-answer. Said as an event, then
                    // closed properly below, so the client reports a failure
                    // rather than hanging on a message that never ends.
                    let _ = sender
                        .send(Ok(encode(&translator.error(
                            "the connection to the model was lost",
                        ))))
                        .await;
                    break;
                }
            };
            buffer.extend_from_slice(&bytes);

            for event in drain_events(&mut buffer) {
                if event.data.trim() == "[DONE]" {
                    continue;
                }
                let Ok(payload) = serde_json::from_str::<Value>(&event.data) else {
                    continue;
                };
                for out in translator.chunk(&payload) {
                    if sender.send(Ok(encode(&out))).await.is_err() {
                        return; // The client hung up.
                    }
                }
            }
        }

        // Always, including after an error: a client waiting for `message_stop`
        // waits forever without it.
        for out in translator.finish() {
            let _ = sender.send(Ok(encode(&out))).await;
        }
    });

    let translated = ReceiverStream::new(receiver);

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/event-stream")
        .header("cache-control", "no-cache")
        .body(Body::from_stream(translated))
        .unwrap_or_else(|_| failed(StatusCode::INTERNAL_SERVER_ERROR, "could not open the stream"))
}

/// One SSE frame. The `event:` line matters — the Messages API is dispatched on
/// it, not on the payload's `type`.
fn encode(event: &gateway::AnthropicEvent) -> String {
    format!("event: {}\ndata: {}\n\n", event.name, event.data)
}

fn failed(status: StatusCode, message: &str) -> Response {
    (status, Json(gateway::error_body("api_error", message))).into_response()
}

fn first_line(text: &str) -> String {
    let trimmed = text.trim();
    let line = trimmed.lines().next().unwrap_or(trimmed);
    line.chars().take(300).collect()
}
