
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

pub async fn messages(State(state): State<SharedState>, Json(body): Json<Value>) -> Response {
    let requested = gateway::requested_model(&body);

    let target = match resolve_target(&state, requested).await {
        Ok(target) => target,
        Err(error) => return failed(StatusCode::BAD_REQUEST, &error.to_string()),
    };

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

    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<String, Infallible>>(64);

    tokio::spawn(async move {
        let mut translator = StreamTranslator::new(model);
        let mut buffer: Vec<u8> = Vec::new();
        let mut source = response.bytes_stream();

        while let Some(chunk) = source.next().await {
            let bytes = match chunk {
                Ok(bytes) => bytes,
                Err(_) => {
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
