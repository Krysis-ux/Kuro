//! Getting JSON-RPC to an MCP server.
//!
//! Two transports, both stateless from the caller's point of view: a call opens
//! the channel, completes the handshake, does its work and closes. That is a
//! deliberate trade. A pooled long-lived connection would save the handshake on
//! every call, but it also means a stdio child process left running for the life
//! of the app, and a user who edits a server's configuration would not see the
//! change until a restart. Reconnecting per call costs a few hundred milliseconds
//! and removes a whole class of stale-state bugs.
//!
//! Cost is contained instead by caching the *tool list* in the manager, which is
//! what the UI reads constantly. Only an actual `tools/call` pays for a connection.

use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::db::{McpServerRecord, McpTransport};
use crate::mcp::protocol;
use crate::{KuroError, Result};

/// A server that has not answered within this is treated as unreachable. Long
/// enough for `npx` to resolve a package on a cold cache, short enough that a
/// dead server does not hold up a conversation.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
/// A single tool call's budget. Search and crawl tools legitimately take a while.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// How a server was reached, and what it said about itself.
#[derive(Debug, Clone, Default)]
pub struct Handshake {
    pub info: protocol::ServerInfo,
    pub tools: Vec<protocol::RemoteTool>,
}

/// Connect, handshake, list tools, disconnect.
pub async fn probe(
    client: &reqwest::Client,
    server: &McpServerRecord,
    bearer: Option<&str>,
) -> Result<Handshake> {
    if !server.is_addressable() {
        return Err(KuroError::bad_request(match server.transport {
            McpTransport::Stdio => "this server has no command to run",
            McpTransport::Http => "this server has no URL",
        }));
    }

    match server.transport {
        McpTransport::Stdio => {
            let mut session = StdioSession::spawn(server)?;
            let outcome = session.handshake().await;
            session.shutdown().await;
            outcome
        }
        McpTransport::Http => {
            let session = HttpSession::open(client, server, bearer).await?;
            session.handshake().await
        }
    }
}

/// Connect, handshake, call one tool, disconnect.
pub async fn call_tool(
    client: &reqwest::Client,
    server: &McpServerRecord,
    bearer: Option<&str>,
    tool: &str,
    arguments: &Value,
) -> Result<(String, bool)> {
    let params = serde_json::json!({ "name": tool, "arguments": arguments });

    match server.transport {
        McpTransport::Stdio => {
            let mut session = StdioSession::spawn(server)?;
            let outcome = stdio_call(&mut session, params, tool).await;
            session.shutdown().await;
            outcome
        }
        McpTransport::Http => {
            let session = HttpSession::open(client, server, bearer).await?;
            session.handshake().await?;
            let result = session
                .call(protocol::METHOD_TOOLS_CALL, params, CALL_TIMEOUT)
                .await
                .map_err(|error| KuroError::other(format!("`{tool}`: {error}")))?;
            Ok(protocol::parse_tool_result(&result))
        }
    }
}

/* ---------- stdio ---------- */

/// Handshake then call, as one step, so the caller can shut the child down
/// afterwards regardless of which half failed.
///
/// The child is always terminated by the caller, including on the error path. A
/// leaked MCP server process would keep running — and keep whatever access it was
/// given — long after the conversation ended.
async fn stdio_call(
    session: &mut StdioSession,
    params: Value,
    tool: &str,
) -> Result<(String, bool)> {
    session.handshake().await?;
    let result = session
        .call(protocol::METHOD_TOOLS_CALL, params, CALL_TIMEOUT)
        .await
        .map_err(|error| KuroError::other(format!("`{tool}`: {error}")))?;
    Ok(protocol::parse_tool_result(&result))
}

struct StdioSession {
    child: Child,
    reader: BufReader<tokio::process::ChildStdout>,
    writer: tokio::process::ChildStdin,
    next_id: u64,
    name: String,
}

impl StdioSession {
    fn spawn(server: &McpServerRecord) -> Result<Self> {
        let command_line = server
            .command
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KuroError::bad_request("this server has no command to run"))?;

        let mut command = Command::new(command_line);
        command.args(&server.args);

        for (key, value) in &server.env {
            if let Some(text) = value.as_str() {
                command.env(key, text);
            }
        }

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The protocol runs on stdout, so a server's own logging on stderr
            // must not be mixed in. Discarding it loses diagnostics; capturing it
            // to the engine log directory would be the next improvement.
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                KuroError::other(format!(
                    "could not start `{command_line}`: {error}. \
                     Check the command is installed and on Kuro's PATH."
                ))
            })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| KuroError::other("the server's stdout was not captured"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| KuroError::other("the server's stdin was not captured"))?;

        Ok(Self {
            child,
            reader: BufReader::new(stdout),
            writer: stdin,
            next_id: 1,
            name: server.name.clone(),
        })
    }

    async fn handshake(&mut self) -> Result<Handshake> {
        let initialize = self
            .call(
                protocol::METHOD_INITIALIZE,
                protocol::initialize_params("kuro", env!("CARGO_PKG_VERSION")),
                HANDSHAKE_TIMEOUT,
            )
            .await?;
        let info = protocol::parse_server_info(&initialize);

        self.notify(protocol::METHOD_INITIALIZED).await?;

        let listed = self
            .call(protocol::METHOD_TOOLS_LIST, serde_json::json!({}), HANDSHAKE_TIMEOUT)
            .await?;

        Ok(Handshake {
            info,
            tools: protocol::parse_tools(&listed),
        })
    }

    async fn notify(&mut self, method: &str) -> Result<()> {
        let payload = protocol::request(None, method, serde_json::json!({}));
        self.write_line(&payload).await
    }

    async fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        self.write_line(&protocol::request(Some(id), method, params))
            .await?;

        let envelope = tokio::time::timeout(timeout, self.read_response(id))
            .await
            .map_err(|_| {
                KuroError::other(format!(
                    "`{}` did not answer {method} within {} seconds",
                    self.name,
                    timeout.as_secs()
                ))
            })??;

        protocol::read_response(&envelope)
    }

    async fn write_line(&mut self, payload: &Value) -> Result<()> {
        let mut line = serde_json::to_string(payload)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Read until the reply to `id` arrives.
    ///
    /// Anything else on the stream — the server's own requests, log
    /// notifications, non-JSON noise from a misbehaving process — is skipped
    /// rather than treated as a protocol violation.
    async fn read_response(&mut self, id: u64) -> Result<Value> {
        let mut line = String::new();

        loop {
            line.clear();
            let read = self.reader.read_line(&mut line).await?;
            if read == 0 {
                return Err(KuroError::other(format!(
                    "`{}` closed the connection without answering",
                    self.name
                )));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Ok(envelope) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            if protocol::is_response_to(&envelope, id) {
                return Ok(envelope);
            }
        }
    }

    async fn shutdown(&mut self) {
        // Closing stdin is how an MCP server is told to exit; killing it outright
        // would deny it the chance to clean up.
        drop(self.writer.shutdown().await);
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

/* ---------- HTTP ---------- */

/// Header a server uses to hand out a session id during `initialize`, which must
/// then be echoed on every later request.
const SESSION_HEADER: &str = "mcp-session-id";

struct HttpSession<'a> {
    client: &'a reqwest::Client,
    url: String,
    headers: HashMap<String, String>,
    session_id: std::sync::Mutex<Option<String>>,
    next_id: std::sync::atomic::AtomicU64,
    name: String,
}

impl<'a> HttpSession<'a> {
    async fn open(
        client: &'a reqwest::Client,
        server: &McpServerRecord,
        bearer: Option<&str>,
    ) -> Result<HttpSession<'a>> {
        let url = server
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| KuroError::bad_request("this server has no URL"))?;

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(KuroError::bad_request(format!(
                "`{url}` is not an http or https URL"
            )));
        }

        let mut headers: HashMap<String, String> = server
            .headers
            .iter()
            .filter_map(|(key, value)| value.as_str().map(|text| (key.clone(), text.to_string())))
            .collect();

        if let Some(token) = bearer.map(str::trim).filter(|token| !token.is_empty()) {
            headers.insert("Authorization".to_string(), format!("Bearer {token}"));
        }

        Ok(HttpSession {
            client,
            url: url.to_string(),
            headers,
            session_id: std::sync::Mutex::new(None),
            next_id: std::sync::atomic::AtomicU64::new(1),
            name: server.name.clone(),
        })
    }

    async fn handshake(&self) -> Result<Handshake> {
        let initialize = self
            .call(
                protocol::METHOD_INITIALIZE,
                protocol::initialize_params("kuro", env!("CARGO_PKG_VERSION")),
                HANDSHAKE_TIMEOUT,
            )
            .await?;
        let info = protocol::parse_server_info(&initialize);

        self.notify(protocol::METHOD_INITIALIZED).await?;

        let listed = self
            .call(protocol::METHOD_TOOLS_LIST, serde_json::json!({}), HANDSHAKE_TIMEOUT)
            .await?;

        Ok(Handshake {
            info,
            tools: protocol::parse_tools(&listed),
        })
    }

    fn build(&self, payload: &Value, timeout: Duration) -> reqwest::RequestBuilder {
        let mut request = self
            .client
            .post(&self.url)
            // Both are advertised because a server may answer a single request
            // either inline as JSON or as a one-event SSE stream, and it chooses.
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .header("MCP-Protocol-Version", protocol::PROTOCOL_VERSION)
            .timeout(timeout)
            .json(payload);

        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        if let Some(session) = self.session_id.lock().ok().and_then(|id| id.clone()) {
            request = request.header("Mcp-Session-Id", session);
        }

        request
    }

    async fn notify(&self, method: &str) -> Result<()> {
        let payload = protocol::request(None, method, serde_json::json!({}));
        // A notification has no reply; a server that answers 202 with no body is
        // behaving correctly, so the response is not inspected beyond errors.
        let response = self
            .build(&payload, HANDSHAKE_TIMEOUT)
            .send()
            .await
            .map_err(|error| self.describe(error))?;

        if response.status().is_client_error() || response.status().is_server_error() {
            return Err(KuroError::other(format!(
                "`{}` rejected {method} with {}",
                self.name,
                response.status()
            )));
        }
        Ok(())
    }

    async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let payload = protocol::request(Some(id), method, params);

        let response = self
            .build(&payload, timeout)
            .send()
            .await
            .map_err(|error| self.describe(error))?;

        if let Some(session) = response
            .headers()
            .get(SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
        {
            if let Ok(mut held) = self.session_id.lock() {
                *held = Some(session.to_string());
            }
        }

        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(http_status_error(&self.name, status, &body));
        }

        let envelope = if content_type.contains("text/event-stream") {
            first_sse_response(&body, id).ok_or_else(|| {
                KuroError::other(format!("`{}` sent a stream with no reply in it", self.name))
            })?
        } else {
            serde_json::from_str::<Value>(&body).map_err(|error| {
                KuroError::other(format!("could not read `{}`'s reply: {error}", self.name))
            })?
        };

        protocol::read_response(&envelope)
    }

    fn describe(&self, error: reqwest::Error) -> KuroError {
        if error.is_timeout() {
            return KuroError::other(format!("`{}` did not answer in time", self.name));
        }
        if error.is_connect() {
            return KuroError::other(format!(
                "could not reach `{}` at {}. Check the URL.",
                self.name, self.url
            ));
        }
        KuroError::other(format!("`{}`: {error}", self.name))
    }
}

/// Turn an HTTP failure into something a user can act on.
fn http_status_error(name: &str, status: reqwest::StatusCode, body: &str) -> KuroError {
    let detail: String = body.trim().chars().take(200).collect();

    match status.as_u16() {
        401 | 403 => KuroError::bad_request(format!(
            "`{name}` refused the credentials ({status}). \
             Check the authorization token, or whether this server needs one at all."
        )),
        404 => KuroError::bad_request(format!(
            "`{name}` has nothing at that URL ({status}). \
             MCP endpoints usually end in `/mcp` or `/sse`."
        )),
        405 => KuroError::bad_request(format!(
            "`{name}` does not accept POST at that URL ({status}), \
             so it is probably not an MCP endpoint."
        )),
        _ => KuroError::other(format!("`{name}` returned {status}: {detail}")),
    }
}

/// Find the JSON-RPC reply to `id` inside an SSE body.
///
/// A streamable-HTTP server answers one request with a short stream: possibly
/// some progress notifications, then the reply. Only the reply is wanted.
fn first_sse_response(body: &str, id: u64) -> Option<Value> {
    for block in body.split("\n\n") {
        let mut data = String::new();
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest.trim_start());
            }
        }
        if data.is_empty() {
            continue;
        }

        if let Ok(envelope) = serde_json::from_str::<Value>(&data) {
            if protocol::is_response_to(&envelope, id) {
                return Some(envelope);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, NewMcpServer};

    fn record(input: NewMcpServer) -> McpServerRecord {
        let db = Db::open_in_memory().expect("open");
        db.insert_mcp_server(&input, None).expect("insert")
    }

    #[tokio::test]
    async fn an_unaddressable_server_fails_before_any_connection_attempt() {
        let server = record(NewMcpServer {
            name: "Nowhere".to_string(),
            transport: "http".to_string(),
            ..Default::default()
        });

        let error = probe(&reqwest::Client::new(), &server, None)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("no URL"), "got: {error}");
    }

    #[tokio::test]
    async fn a_stdio_server_with_no_command_is_reported_as_such() {
        let server = record(NewMcpServer {
            name: "Nothing".to_string(),
            transport: "stdio".to_string(),
            ..Default::default()
        });

        let error = probe(&reqwest::Client::new(), &server, None)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("no command"), "got: {error}");
    }

    #[tokio::test]
    async fn a_non_http_url_is_rejected_with_a_reason() {
        let server = record(NewMcpServer {
            name: "Odd".to_string(),
            transport: "http".to_string(),
            url: Some("ftp://example.com/mcp".to_string()),
            ..Default::default()
        });

        let error = probe(&reqwest::Client::new(), &server, None)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("not an http"), "got: {error}");
    }

    #[tokio::test]
    async fn a_command_that_does_not_exist_says_how_to_fix_it() {
        let server = record(NewMcpServer {
            name: "Missing".to_string(),
            transport: "stdio".to_string(),
            command: Some("kuro-definitely-not-a-real-binary".to_string()),
            ..Default::default()
        });

        let error = probe(&reqwest::Client::new(), &server, None)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("PATH"), "the message should suggest a cause: {error}");
    }

    #[tokio::test]
    async fn a_stdio_server_that_answers_the_handshake_yields_its_tools() {
        // `cat` echoes back whatever is written to it, so it cannot complete a
        // handshake — but it does prove the spawn, write and read path works and
        // that a non-answer is reported rather than hanging forever.
        let server = record(NewMcpServer {
            name: "Echo".to_string(),
            transport: "stdio".to_string(),
            command: Some("cat".to_string()),
            ..Default::default()
        });

        let outcome = probe(&reqwest::Client::new(), &server, None).await;

        assert!(outcome.is_err(), "an echo is not a valid MCP server");
    }

    #[test]
    fn reads_a_reply_out_of_an_sse_body() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n";
        let envelope = first_sse_response(body, 1).expect("reply");
        assert!(envelope["result"].get("tools").is_some());
    }

    #[test]
    fn skips_notifications_that_precede_the_reply() {
        let body = concat!(
            "data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\n",
            "data: {\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{\"ok\":true}}\n\n",
        );

        let envelope = first_sse_response(body, 9).expect("reply");

        assert_eq!(envelope["result"]["ok"], true);
    }

    #[test]
    fn ignores_a_reply_to_a_different_request() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}\n\n";
        assert!(first_sse_response(body, 1).is_none());
    }

    #[test]
    fn handles_a_data_payload_split_across_lines() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"id\":1,\ndata: \"result\":{\"ok\":true}}\n\n";
        let envelope = first_sse_response(body, 1).expect("reply");
        assert_eq!(envelope["result"]["ok"], true);
    }

    #[test]
    fn an_empty_stream_yields_nothing_rather_than_panicking() {
        assert!(first_sse_response("", 1).is_none());
        assert!(first_sse_response("event: ping\n\n", 1).is_none());
    }

    #[test]
    fn http_failures_are_translated_into_advice() {
        let unauthorized =
            http_status_error("Exa", reqwest::StatusCode::UNAUTHORIZED, "").to_string();
        assert!(unauthorized.contains("token"), "got: {unauthorized}");

        let missing = http_status_error("Exa", reqwest::StatusCode::NOT_FOUND, "").to_string();
        assert!(missing.contains("/mcp"), "got: {missing}");

        let wrong_method =
            http_status_error("Exa", reqwest::StatusCode::METHOD_NOT_ALLOWED, "").to_string();
        assert!(wrong_method.contains("not an MCP endpoint"), "got: {wrong_method}");

        let other = http_status_error("Exa", reqwest::StatusCode::BAD_GATEWAY, "upstream died");
        assert!(other.to_string().contains("upstream died"));
    }

    #[test]
    fn a_long_error_body_is_truncated_into_the_message() {
        let error = http_status_error(
            "Exa",
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            &"x".repeat(5000),
        )
        .to_string();
        assert!(error.len() < 400, "an error message must stay readable");
    }
}
