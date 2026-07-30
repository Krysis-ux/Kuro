//! Storage for MCP tool servers.
//!
//! A row here is a *configuration*, not a connection. Whether a server is
//! currently reachable is decided by the manager at connect time; `status` and
//! `tool_count` are the cached result of the last attempt so the list page can
//! render without dialling every server.

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{json_or, now, Db};
use crate::{KuroError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// A local process speaking JSON-RPC over its own stdin and stdout.
    Stdio,
    /// A remote endpoint speaking JSON-RPC over HTTP, answering either with
    /// JSON or with an SSE stream.
    Http,
}

impl McpTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
        }
    }

    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "stdio" => Ok(Self::Stdio),
            // `http_sse` is what version 1 of the schema called it.
            "http" | "http_sse" | "sse" => Ok(Self::Http),
            other => Err(KuroError::bad_request(format!(
                "unknown MCP transport `{other}`; expected `stdio` or `http`"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpStatus {
    Connected,
    Disconnected,
    Error,
}

impl McpStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::Error => "error",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "connected" => Self::Connected,
            "error" => Self::Error,
            _ => Self::Disconnected,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerRecord {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: Map<String, Value>,
    pub url: Option<String>,
    pub headers: Map<String, Value>,
    pub enabled: bool,
    pub status: McpStatus,
    pub last_error: Option<String>,
    /// Set when the server was installed from the recommended list.
    pub slug: Option<String>,
    pub tool_count: Option<i64>,
    /// Reference into the credential store for this server's bearer token. The
    /// token is never part of this record.
    pub auth_ref: Option<String>,
    pub created_at: String,
}

impl McpServerRecord {
    /// True when the configuration is complete enough to attempt a connection.
    pub fn is_addressable(&self) -> bool {
        match self.transport {
            McpTransport::Stdio => self
                .command
                .as_deref()
                .map(|command| !command.trim().is_empty())
                .unwrap_or(false),
            McpTransport::Http => self
                .url
                .as_deref()
                .map(|url| !url.trim().is_empty())
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewMcpServer {
    pub name: String,
    pub transport: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Map<String, Value>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: Map<String, Value>,
    #[serde(default)]
    pub slug: Option<String>,
    /// Bearer token, held only long enough to move it into the credential store.
    #[serde(default)]
    pub auth_token: Option<String>,
}

impl Db {
    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerRecord>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS} FROM mcp_servers ORDER BY created_at"
            ))?;
            let rows = stmt
                .query_map([], read_server)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_mcp_server(&self, id: &str) -> Result<Option<McpServerRecord>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    &format!("SELECT {COLUMNS} FROM mcp_servers WHERE id = ?1"),
                    params![id],
                    read_server,
                )
                .optional()?)
        })
    }

    /// Insert a server. `auth_ref` is decided by the caller, which owns the
    /// credential store; this layer only records the reference.
    pub fn insert_mcp_server(
        &self,
        input: &NewMcpServer,
        auth_ref: Option<&str>,
    ) -> Result<McpServerRecord> {
        let transport = McpTransport::parse(input.transport.trim())?;
        let name = input.name.trim();
        if name.is_empty() {
            return Err(KuroError::bad_request("the server needs a name"));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now();

        self.with(|conn| {
            conn.execute(
                "INSERT INTO mcp_servers
                     (id, name, transport, command, args, env, url, headers,
                      enabled, status, last_error, created_at, slug, tool_count, auth_ref)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 'disconnected', NULL, ?9, ?10, NULL, ?11)",
                params![
                    id,
                    name,
                    transport.as_str(),
                    input.command.as_deref().map(str::trim).filter(|c| !c.is_empty()),
                    serde_json::to_string(&input.args)?,
                    serde_json::to_string(&input.env)?,
                    input.url.as_deref().map(str::trim).filter(|u| !u.is_empty()),
                    serde_json::to_string(&input.headers)?,
                    created_at,
                    input.slug.as_deref(),
                    auth_ref,
                ],
            )?;
            Ok(())
        })?;

        self.get_mcp_server(&id)?
            .ok_or_else(|| KuroError::other("the server disappeared immediately after insert"))
    }

    /// Point a server at its entry in the credential store.
    ///
    /// Separate from insert because the reference is derived from the id, which
    /// does not exist until the row does.
    pub fn set_mcp_auth_ref(&self, id: &str, auth_ref: Option<&str>) -> Result<()> {
        self.with(|conn| {
            let changed = conn.execute(
                "UPDATE mcp_servers SET auth_ref = ?2 WHERE id = ?1",
                params![id, auth_ref],
            )?;
            if changed == 0 {
                return Err(KuroError::not_found(format!("MCP server `{id}`")));
            }
            Ok(())
        })
    }

    pub fn set_mcp_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        self.with(|conn| {
            let changed = conn.execute(
                "UPDATE mcp_servers SET enabled = ?2 WHERE id = ?1",
                params![id, enabled as i64],
            )?;
            if changed == 0 {
                return Err(KuroError::not_found(format!("MCP server `{id}`")));
            }
            Ok(())
        })
    }

    /// Record the outcome of a connection attempt.
    pub fn set_mcp_status(
        &self,
        id: &str,
        status: McpStatus,
        tool_count: Option<i64>,
        error: Option<&str>,
    ) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE mcp_servers
                    SET status = ?2, tool_count = ?3, last_error = ?4
                  WHERE id = ?1",
                params![id, status.as_str(), tool_count, error],
            )?;
            Ok(())
        })
    }

    pub fn delete_mcp_server(&self, id: &str) -> Result<bool> {
        self.with(|conn| {
            let removed = conn.execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
            Ok(removed > 0)
        })
    }
}

const COLUMNS: &str = "id, name, transport, command, args, env, url, headers, \
                       enabled, status, last_error, created_at, slug, tool_count, auth_ref";

fn read_server(row: &Row<'_>) -> rusqlite::Result<McpServerRecord> {
    let transport: String = row.get(2)?;
    let args: Option<String> = row.get(4)?;
    let env: Option<String> = row.get(5)?;
    let headers: Option<String> = row.get(7)?;
    let status: String = row.get(9)?;

    Ok(McpServerRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        // A transport string this build does not recognise is treated as stdio
        // rather than failing the whole list; the row is still editable and
        // deletable in the UI.
        transport: McpTransport::parse(&transport).unwrap_or(McpTransport::Stdio),
        command: row.get(3)?,
        args: json_or(args.as_deref()),
        env: json_or(env.as_deref()),
        url: row.get(6)?,
        headers: json_or(headers.as_deref()),
        enabled: row.get::<_, i64>(8)? != 0,
        status: McpStatus::parse(&status),
        last_error: row.get(10)?,
        created_at: row.get(11)?,
        slug: row.get(12)?,
        tool_count: row.get(13)?,
        auth_ref: row.get(14)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http_server(name: &str) -> NewMcpServer {
        NewMcpServer {
            name: name.to_string(),
            transport: "http".to_string(),
            url: Some("https://mcp.example.com/mcp".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn inserts_and_reads_back_an_http_server() {
        let db = Db::open_in_memory().expect("open");
        let created = db
            .insert_mcp_server(&http_server("Example"), Some("mcp:abc"))
            .expect("insert");

        assert_eq!(created.name, "Example");
        assert_eq!(created.transport, McpTransport::Http);
        assert!(created.enabled);
        assert_eq!(created.status, McpStatus::Disconnected);
        assert_eq!(created.auth_ref.as_deref(), Some("mcp:abc"));
        assert!(created.is_addressable());

        let listed = db.list_mcp_servers().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);
    }

    #[test]
    fn stores_args_env_and_headers_as_structured_values() {
        let db = Db::open_in_memory().expect("open");
        let mut input = NewMcpServer {
            name: "Files".to_string(),
            transport: "stdio".to_string(),
            command: Some("npx".to_string()),
            args: vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string()],
            ..Default::default()
        };
        input.env.insert("ROOT".to_string(), Value::String("/tmp".to_string()));

        let created = db.insert_mcp_server(&input, None).expect("insert");

        assert_eq!(created.args.len(), 2);
        assert_eq!(created.env["ROOT"], Value::String("/tmp".to_string()));
        assert!(created.is_addressable());
    }

    #[test]
    fn a_stdio_server_without_a_command_is_not_addressable() {
        let db = Db::open_in_memory().expect("open");
        let created = db
            .insert_mcp_server(
                &NewMcpServer {
                    name: "Broken".to_string(),
                    transport: "stdio".to_string(),
                    ..Default::default()
                },
                None,
            )
            .expect("insert");

        assert!(!created.is_addressable());
    }

    #[test]
    fn rejects_a_blank_name_and_an_unknown_transport() {
        let db = Db::open_in_memory().expect("open");

        let blank = db.insert_mcp_server(&http_server("   "), None);
        assert!(blank.is_err());

        let unknown = db.insert_mcp_server(
            &NewMcpServer {
                name: "X".to_string(),
                transport: "carrier-pigeon".to_string(),
                ..Default::default()
            },
            None,
        );
        assert!(unknown.unwrap_err().to_string().contains("transport"));
    }

    #[test]
    fn accepts_the_version_one_transport_spelling() {
        assert_eq!(McpTransport::parse("http_sse").expect("parse"), McpTransport::Http);
    }

    #[test]
    fn toggling_and_status_updates_persist() {
        let db = Db::open_in_memory().expect("open");
        let created = db.insert_mcp_server(&http_server("Example"), None).expect("insert");

        db.set_mcp_enabled(&created.id, false).expect("disable");
        db.set_mcp_status(&created.id, McpStatus::Connected, Some(7), None)
            .expect("status");

        let reloaded = db.get_mcp_server(&created.id).expect("get").expect("present");
        assert!(!reloaded.enabled);
        assert_eq!(reloaded.status, McpStatus::Connected);
        assert_eq!(reloaded.tool_count, Some(7));
        assert_eq!(reloaded.last_error, None);
    }

    #[test]
    fn a_failed_connection_keeps_its_reason() {
        let db = Db::open_in_memory().expect("open");
        let created = db.insert_mcp_server(&http_server("Example"), None).expect("insert");

        db.set_mcp_status(&created.id, McpStatus::Error, None, Some("connection refused"))
            .expect("status");

        let reloaded = db.get_mcp_server(&created.id).expect("get").expect("present");
        assert_eq!(reloaded.status, McpStatus::Error);
        assert_eq!(reloaded.last_error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn enabling_a_missing_server_is_an_error_not_a_silent_no_op() {
        let db = Db::open_in_memory().expect("open");
        assert!(db.set_mcp_enabled("nope", true).is_err());
    }

    #[test]
    fn deleting_reports_whether_anything_was_removed() {
        let db = Db::open_in_memory().expect("open");
        let created = db.insert_mcp_server(&http_server("Example"), None).expect("insert");

        assert!(db.delete_mcp_server(&created.id).expect("delete"));
        assert!(!db.delete_mcp_server(&created.id).expect("second delete"));
        assert!(db.list_mcp_servers().expect("list").is_empty());
    }
}
