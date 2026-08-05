//! `kuro run` — chat with a model from the terminal.

use std::io::{IsTerminal, Write};

use anyhow::{bail, Result};
use futures::StreamExt;
use kuro_core::sse::drain_events;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::client::KuroClient;
use crate::commands::models::resolve_model;

/// Dim text, used for anything that is not model output.
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// Frames of the waiting spinner.
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// A spinner covering the wait before the first token.
///
/// That gap is not small and it is not always short. A local model has to load,
/// a reasoning model thinks for a while before it says anything, and a free
/// provider may be failing over between two endpoints — and until now every one
/// of those looked identical from the terminal: a cursor on an empty line, with
/// no way to tell a slow answer from a hung one.
///
/// On stderr, so that `kuro run qwen "…" > out.txt` still writes clean output.
/// Skipped entirely when stderr is not a terminal, because escape codes in a
/// log file are noise nobody asked for.
struct Thinking(Option<tokio::task::JoinHandle<()>>);

impl Thinking {
    fn start() -> Self {
        if !std::io::stderr().is_terminal() {
            return Self(None);
        }

        Self(Some(tokio::spawn(async move {
            let mut frame = 0usize;
            loop {
                eprint!("\r{DIM}{} thinking…{RESET}", SPINNER[frame % SPINNER.len()]);
                let _ = std::io::stderr().flush();
                frame += 1;
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            }
        })))
    }

    /// Stop and wipe the line, so the answer starts at column zero.
    fn stop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
            eprint!("\r\x1b[2K");
            let _ = std::io::stderr().flush();
        }
    }
}

// A `?` anywhere in the streaming loop would otherwise leave the spinner
// running over whatever error is printed next.
impl Drop for Thinking {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn run(
    client: &KuroClient,
    model: Option<String>,
    prompt: Vec<String>,
    effort: Option<String>,
) -> Result<()> {
    client.ensure_running().await?;

    let model_id = resolve_model(client, model).await?;

    // Load before the first prompt so the wait is explained rather than
    // appearing as an unresponsive terminal.
    eprint!("{DIM}Loading {model_id}…{RESET}");
    let _ = std::io::stderr().flush();
    client
        .post(&format!("/api/models/{model_id}/load"), json!({}))
        .await?;
    eprint!("\r\x1b[2K");
    let _ = std::io::stderr().flush();

    let conversation = client
        .post("/api/conversations", json!({ "model_id": model_id }))
        .await?;
    let conversation_id = conversation["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("the server did not return a conversation id"))?
        .to_string();

    // One-shot mode: `kuro run qwen3-4b "explain gravity"`.
    if !prompt.is_empty() {
        return send(client, &conversation_id, &prompt.join(" "), effort.as_deref()).await;
    }

    println!("{DIM}{model_id} · /bye to exit{RESET}\n");

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        print!("{DIM}>{RESET} ");
        let _ = std::io::stdout().flush();

        let Some(line) = lines.next_line().await? else {
            println!();
            break; // Ctrl-D
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if matches!(line, "/bye" | "/exit" | "/quit") {
            break;
        }

        if let Err(error) = send(client, &conversation_id, line, effort.as_deref()).await {
            eprintln!("{DIM}error: {error}{RESET}\n");
        }
    }

    Ok(())
}

/// Send one message and print the reply as it streams.
async fn send(
    client: &KuroClient,
    conversation_id: &str,
    content: &str,
    effort: Option<&str>,
) -> Result<()> {
    let mut body = json!({ "content": content });
    if let Some(effort) = effort {
        body["effort"] = json!(effort);
    }

    let response = client
        .http()
        .post(client.url(&format!("/api/conversations/{conversation_id}/messages")))
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|value| {
                value["error"]["message"]
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("request failed with status {status}"));
        bail!("{message}");
    }

    let mut buffer: Vec<u8> = Vec::new();
    let mut stream = response.bytes_stream();
    let mut wrote_output = false;
    // Runs until the first thing worth showing arrives, and is wiped by `Drop`
    // on any path out of this loop.
    let mut thinking = Thinking::start();

    while let Some(chunk) = stream.next().await {
        buffer.extend_from_slice(&chunk?);

        for event in drain_events(&mut buffer) {
            let Ok(payload) = serde_json::from_str::<Value>(&event.data) else {
                continue;
            };

            match event.event.as_deref() {
                Some("token") => {
                    if let Some(text) = payload["content"].as_str() {
                        thinking.stop();
                        print!("{text}");
                        let _ = std::io::stdout().flush();
                        wrote_output = true;
                    }
                }
                Some("done") => {
                    thinking.stop();
                    if wrote_output {
                        println!();
                    }
                    print_stats(&payload);
                }
                Some("error") => {
                    thinking.stop();
                    if wrote_output {
                        println!();
                    }
                    bail!(
                        "{}",
                        payload["message"].as_str().unwrap_or("generation failed")
                    );
                }
                // `reasoning` is intentionally not printed: the terminal shows
                // the answer, and thinking is available in the web interface.
                _ => {}
            }
        }
    }

    println!();
    Ok(())
}

/// One dim line of generation statistics, the terminal's request inspector.
fn print_stats(payload: &Value) {
    let mut parts: Vec<String> = Vec::new();

    if let Some(tokens) = payload["usage"]["completionTokens"].as_i64() {
        parts.push(format!("{tokens} tokens"));
    }
    if let Some(rate) = payload["timings"]["tokensPerSecond"].as_f64() {
        parts.push(format!("{rate:.1} tok/s"));
    }
    if let Some(ttft) = payload["timings"]["ttftMs"].as_i64() {
        parts.push(format!("{ttft} ms to first token"));
    }

    if !parts.is_empty() {
        println!("{DIM}{}{RESET}", parts.join(" · "));
    }
}
