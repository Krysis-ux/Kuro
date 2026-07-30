//! The wire format of the Model Context Protocol.
//!
//! MCP is JSON-RPC 2.0 with a fixed opening exchange: the client sends
//! `initialize`, the server answers with what it supports, the client confirms
//! with an `initialized` notification, and only then are `tools/list` and
//! `tools/call` allowed. Getting that order wrong is the most common reason a
//! server refuses to talk, so the handshake is expressed here rather than being
//! open-coded in each transport.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::{KuroError, Result};

/// Protocol revision Kuro speaks. A server that only supports an older one
/// answers with that version, which is accepted — the surface Kuro uses (tools)
/// has not changed across these revisions.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_INITIALIZED: &str = "notifications/initialized";
pub const METHOD_TOOLS_LIST: &str = "tools/list";
pub const METHOD_TOOLS_CALL: &str = "tools/call";

/// A JSON-RPC request. `id` is absent for a notification, which is how the
/// protocol distinguishes "answer me" from "just so you know".
pub fn request(id: Option<u64>, method: &str, params: Value) -> Value {
    match id {
        Some(id) => json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        None => json!({ "jsonrpc": "2.0", "method": method, "params": params }),
    }
}

/// The `initialize` parameters.
pub fn initialize_params(client_name: &str, client_version: &str) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        // Kuro consumes tools; it does not offer sampling or roots back to the
        // server, and says so rather than claiming capabilities it lacks.
        "capabilities": { "tools": {} },
        "clientInfo": { "name": client_name, "version": client_version },
    })
}

/// What a server said about itself during the handshake.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerInfo {
    pub name: Option<String>,
    pub version: Option<String>,
    pub protocol_version: Option<String>,
    /// Whether the server declared a `tools` capability. A server without one has
    /// nothing Kuro can use, which is worth saying plainly.
    pub supports_tools: bool,
}

pub fn parse_server_info(result: &Value) -> ServerInfo {
    ServerInfo {
        name: result
            .pointer("/serverInfo/name")
            .and_then(Value::as_str)
            .map(str::to_string),
        version: result
            .pointer("/serverInfo/version")
            .and_then(Value::as_str)
            .map(str::to_string),
        protocol_version: result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .map(str::to_string),
        supports_tools: result.pointer("/capabilities/tools").is_some(),
    }
}

/// One tool as the server describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RemoteTool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// JSON Schema. Absent on servers that take no arguments.
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Option<Value>,
    /// Optional human-facing name, newer than `name` and not always present.
    #[serde(default)]
    pub title: Option<String>,
}

impl RemoteTool {
    /// A schema an engine will accept.
    ///
    /// Engines reject a `parameters` value that is not an object schema, and
    /// servers do sometimes omit `inputSchema` entirely, so a valid empty schema
    /// is substituted rather than passing `null` through.
    pub fn schema(&self) -> Value {
        match &self.input_schema {
            Some(Value::Object(map)) if map.contains_key("type") => Value::Object(map.clone()),
            Some(Value::Object(map)) => {
                let mut with_type = map.clone();
                with_type.insert("type".to_string(), json!("object"));
                Value::Object(with_type)
            }
            _ => json!({ "type": "object", "properties": {} }),
        }
    }

    pub fn describe(&self) -> String {
        self.description
            .clone()
            .or_else(|| self.title.clone())
            .unwrap_or_else(|| format!("The `{}` tool, provided by an MCP server.", self.name))
    }
}

/// Read the `tools` array out of a `tools/list` result.
pub fn parse_tools(result: &Value) -> Vec<RemoteTool> {
    result
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                // A single malformed entry should cost that one tool, not the
                // whole server.
                .filter_map(|tool| serde_json::from_value::<RemoteTool>(tool.clone()).ok())
                .filter(|tool| !tool.name.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Flatten a `tools/call` result into text.
///
/// The result is a list of content blocks which may be text, images, or embedded
/// resources. Only text is usable by a text model, so the rest is described
/// rather than dropped silently — a model told "[image]" can at least say so.
pub fn parse_tool_result(result: &Value) -> (String, bool) {
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Newer servers may return a structured payload instead of content blocks.
    if let Some(structured) = result.get("structuredContent").filter(|v| !v.is_null()) {
        let rendered = serde_json::to_string_pretty(structured).unwrap_or_else(|_| structured.to_string());
        return (rendered, is_error);
    }

    let Some(blocks) = result.get("content").and_then(Value::as_array) else {
        // No content at all is a valid answer for a tool whose whole job is a
        // side effect.
        return ("(the tool returned no content)".to_string(), is_error);
    };

    let mut parts: Vec<String> = Vec::new();
    for block in blocks {
        let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "text" => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    parts.push(text.to_string());
                }
            }
            "image" => parts.push("[an image, which this model cannot read]".to_string()),
            "audio" => parts.push("[audio, which this model cannot read]".to_string()),
            "resource" | "resource_link" => {
                let uri = block
                    .pointer("/resource/uri")
                    .or_else(|| block.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                // An embedded resource often carries its text inline.
                match block.pointer("/resource/text").and_then(Value::as_str) {
                    Some(text) => parts.push(format!("{uri}:\n{text}")),
                    None => parts.push(format!("[resource: {uri}]")),
                }
            }
            other => parts.push(format!("[unsupported content: {other}]")),
        }
    }

    let joined = parts.join("\n").trim().to_string();
    if joined.is_empty() {
        return ("(the tool returned no readable content)".to_string(), is_error);
    }
    (joined, is_error)
}

/// Turn a JSON-RPC envelope into either its result or a described error.
pub fn read_response(envelope: &Value) -> Result<Value> {
    if let Some(error) = envelope.get("error").filter(|value| !value.is_null()) {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the server reported an error with no message");
        let code = error.get("code").and_then(Value::as_i64);

        return Err(KuroError::other(match code {
            Some(code) => format!("{message} (code {code})"),
            None => message.to_string(),
        }));
    }

    envelope
        .get("result")
        .cloned()
        .ok_or_else(|| KuroError::other("the server's reply had neither a result nor an error"))
}

/// Whether an envelope is the answer to the request that carried `id`.
///
/// Servers interleave notifications and requests of their own with their replies,
/// so a reader has to skip anything that is not the response it is waiting for.
pub fn is_response_to(envelope: &Value, id: u64) -> bool {
    envelope.get("id").and_then(Value::as_u64) == Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_carries_an_id_and_a_notification_does_not() {
        let call = request(Some(7), METHOD_TOOLS_LIST, json!({}));
        assert_eq!(call["jsonrpc"], "2.0");
        assert_eq!(call["id"], 7);
        assert_eq!(call["method"], "tools/list");

        let note = request(None, METHOD_INITIALIZED, json!({}));
        assert!(note.get("id").is_none(), "a notification must not be answerable");
    }

    #[test]
    fn the_handshake_declares_only_what_kuro_actually_supports() {
        let params = initialize_params("kuro", "0.1.0");
        assert_eq!(params["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(params["clientInfo"]["name"], "kuro");
        assert!(params["capabilities"].get("tools").is_some());
        assert!(
            params["capabilities"].get("sampling").is_none(),
            "Kuro does not offer sampling back to a server"
        );
    }

    #[test]
    fn reads_what_a_server_says_about_itself() {
        let result = json!({
            "protocolVersion": "2025-03-26",
            "serverInfo": { "name": "Exa", "version": "1.2.0" },
            "capabilities": { "tools": {} },
        });

        let info = parse_server_info(&result);

        assert_eq!(info.name.as_deref(), Some("Exa"));
        assert_eq!(info.version.as_deref(), Some("1.2.0"));
        assert_eq!(info.protocol_version.as_deref(), Some("2025-03-26"));
        assert!(info.supports_tools);
    }

    #[test]
    fn a_server_with_no_tools_capability_is_recognised() {
        let info = parse_server_info(&json!({ "capabilities": { "resources": {} } }));
        assert!(!info.supports_tools);
        assert_eq!(info.name, None);
    }

    #[test]
    fn reads_a_tools_list() {
        let result = json!({
            "tools": [
                {
                    "name": "web_search_exa",
                    "description": "Search the web",
                    "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } } },
                },
                { "name": "crawl" },
            ]
        });

        let tools = parse_tools(&result);

        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "web_search_exa");
        assert_eq!(tools[0].schema()["properties"]["query"]["type"], "string");
        assert!(tools[1].describe().contains("crawl"), "a tool with no description still gets one");
    }

    #[test]
    fn a_malformed_tool_entry_does_not_discard_its_neighbours() {
        let result = json!({ "tools": [ { "no_name_field": true }, { "name": "good" } ] });
        let tools = parse_tools(&result);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "good");
    }

    #[test]
    fn a_missing_tools_array_is_an_empty_list_not_an_error() {
        assert!(parse_tools(&json!({})).is_empty());
    }

    #[test]
    fn a_schema_without_a_type_is_repaired_rather_than_passed_on() {
        let tool = RemoteTool {
            name: "x".to_string(),
            description: None,
            input_schema: Some(json!({ "properties": { "a": { "type": "string" } } })),
            title: None,
        };

        assert_eq!(tool.schema()["type"], "object", "engines reject a schema with no type");
        assert!(tool.schema()["properties"].get("a").is_some());
    }

    #[test]
    fn a_tool_with_no_schema_gets_a_valid_empty_one() {
        let tool = RemoteTool {
            name: "x".to_string(),
            description: None,
            input_schema: None,
            title: None,
        };
        assert_eq!(tool.schema(), json!({ "type": "object", "properties": {} }));
    }

    #[test]
    fn flattens_text_content_blocks() {
        let result = json!({
            "content": [ { "type": "text", "text": "first" }, { "type": "text", "text": "second" } ]
        });

        let (text, is_error) = parse_tool_result(&result);

        assert_eq!(text, "first\nsecond");
        assert!(!is_error);
    }

    #[test]
    fn a_tool_error_is_carried_through_with_its_message() {
        let result = json!({ "isError": true, "content": [ { "type": "text", "text": "rate limited" } ] });
        let (text, is_error) = parse_tool_result(&result);

        assert!(is_error);
        assert_eq!(text, "rate limited");
    }

    #[test]
    fn unreadable_content_is_described_rather_than_dropped() {
        let result = json!({ "content": [ { "type": "image", "data": "…" } ] });
        let (text, _) = parse_tool_result(&result);
        assert!(text.contains("image"), "the model should know something was returned");
    }

    #[test]
    fn an_embedded_resource_uses_its_inline_text_when_there_is_some() {
        let result = json!({
            "content": [ {
                "type": "resource",
                "resource": { "uri": "file:///notes.md", "text": "the contents" }
            } ]
        });

        let (text, _) = parse_tool_result(&result);

        assert!(text.contains("file:///notes.md"));
        assert!(text.contains("the contents"));
    }

    #[test]
    fn a_structured_result_is_preferred_over_content_blocks() {
        let result = json!({ "structuredContent": { "temperature": 21 } });
        let (text, _) = parse_tool_result(&result);
        assert!(text.contains("temperature"));
        assert!(text.contains("21"));
    }

    #[test]
    fn an_empty_result_says_so_instead_of_returning_a_blank() {
        let (text, _) = parse_tool_result(&json!({ "content": [] }));
        assert!(!text.trim().is_empty());
    }

    #[test]
    fn a_result_envelope_yields_its_result() {
        let envelope = json!({ "jsonrpc": "2.0", "id": 1, "result": { "tools": [] } });
        assert!(read_response(&envelope).expect("result").get("tools").is_some());
    }

    #[test]
    fn an_error_envelope_becomes_a_readable_message() {
        let envelope = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": { "code": -32601, "message": "Method not found" },
        });

        let error = read_response(&envelope).unwrap_err().to_string();

        assert!(error.contains("Method not found"));
        assert!(error.contains("-32601"), "the code helps when the message is vague");
    }

    #[test]
    fn an_envelope_with_neither_result_nor_error_is_an_error() {
        assert!(read_response(&json!({ "jsonrpc": "2.0", "id": 1 })).is_err());
    }

    #[test]
    fn only_the_matching_id_counts_as_a_response() {
        assert!(is_response_to(&json!({ "id": 4, "result": {} }), 4));
        assert!(!is_response_to(&json!({ "id": 5, "result": {} }), 4));
        assert!(
            !is_response_to(&json!({ "method": "notifications/message" }), 4),
            "a server notification is not a reply"
        );
    }
}
