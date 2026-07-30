//! Model Context Protocol client.
//!
//! Kuro is an MCP *host*: it connects out to servers the user has added and hands
//! their tools to whichever model is running. It is not a server itself.
//!
//! * [`protocol`] is the wire format.
//! * [`transport`] gets JSON-RPC to a server over stdio or HTTP.
//! * [`registry`] is the recommended list shown in the interface.
//! * [`McpManager`] is what the rest of the application talks to.

pub mod protocol;
pub mod registry;
pub mod transport;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::db::{Db, McpServerRecord, McpStatus};
use crate::secrets::SecretStore;
use crate::tools::{sanitize_tool_name, unique_tool_name, ToolOrigin, ToolSpec};
use crate::{KuroError, Result};

/// How long a tool list is trusted before the server is asked again.
///
/// Long enough that opening the tools page repeatedly costs nothing; short enough
/// that a server which gained a tool is picked up without a restart.
const TOOL_CACHE_TTL: Duration = Duration::from_secs(300);

/// Prefix for a server's credential reference. Namespaced so an MCP token and a
/// provider key can never collide in the credential store.
fn auth_reference(server_id: &str) -> String {
    format!("mcp:{server_id}")
}

struct CachedTools {
    tools: Vec<protocol::RemoteTool>,
    fetched_at: Instant,
}

/// One server, with the tools it exposes, as the UI shows it.
#[derive(Debug, Clone, Serialize)]
pub struct ServerWithTools {
    #[serde(flatten)]
    pub server: McpServerRecord,
    pub tools: Vec<ExposedTool>,
    /// Whether a bearer token is stored. Never the token.
    pub has_auth: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExposedTool {
    /// The name the model is given, after collision handling.
    pub name: String,
    /// The name the server itself uses.
    pub remote_name: String,
    pub description: String,
}

pub struct McpManager {
    db: Db,
    secrets: SecretStore,
    client: reqwest::Client,
    cache: Mutex<HashMap<String, CachedTools>>,
}

impl McpManager {
    pub fn new(db: Db, secrets: SecretStore, client: reqwest::Client) -> Self {
        Self {
            db,
            secrets,
            client,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Store a server's bearer token, returning the reference to record.
    pub fn store_auth(&self, server_id: &str, token: &str) -> Result<String> {
        let reference = auth_reference(server_id);
        self.secrets.put(&reference, token)?;
        Ok(reference)
    }

    pub fn forget_auth(&self, server: &McpServerRecord) -> Result<()> {
        if let Some(reference) = &server.auth_ref {
            self.secrets.delete(reference)?;
        }
        Ok(())
    }

    fn bearer_for(&self, server: &McpServerRecord) -> Option<String> {
        server
            .auth_ref
            .as_deref()
            .and_then(|reference| self.secrets.get(reference).ok().flatten())
    }

    /// Connect to a server, record the outcome, and cache what it exposes.
    ///
    /// The result is stored on the row whether it succeeded or failed, so the
    /// interface can show *why* a server is not working without the user having
    /// to press anything.
    pub async fn refresh(&self, server_id: &str) -> Result<Vec<protocol::RemoteTool>> {
        let server = self
            .db
            .get_mcp_server(server_id)?
            .ok_or_else(|| KuroError::not_found(format!("MCP server `{server_id}`")))?;

        let bearer = self.bearer_for(&server);
        let outcome = transport::probe(&self.client, &server, bearer.as_deref()).await;

        match outcome {
            Ok(handshake) => {
                let count = handshake.tools.len() as i64;
                self.db
                    .set_mcp_status(&server.id, McpStatus::Connected, Some(count), None)?;

                self.cache.lock().await.insert(
                    server.id.clone(),
                    CachedTools {
                        tools: handshake.tools.clone(),
                        fetched_at: Instant::now(),
                    },
                );

                if handshake.tools.is_empty() && !handshake.info.supports_tools {
                    tracing::info!(
                        server = %server.name,
                        "connected but the server exposes no tools"
                    );
                }

                Ok(handshake.tools)
            }
            Err(error) => {
                let message = error.to_string();
                self.db
                    .set_mcp_status(&server.id, McpStatus::Error, None, Some(&message))?;
                self.cache.lock().await.remove(&server.id);
                Err(error)
            }
        }
    }

    /// Tools for one server, from cache when it is fresh.
    async fn tools_for(&self, server: &McpServerRecord) -> Vec<protocol::RemoteTool> {
        if let Some(cached) = self.cache.lock().await.get(&server.id) {
            if cached.fetched_at.elapsed() < TOOL_CACHE_TTL {
                return cached.tools.clone();
            }
        }

        match self.refresh(&server.id).await {
            Ok(tools) => tools,
            Err(error) => {
                // A server being down must not stop the other servers' tools from
                // reaching the model, so this is logged and skipped.
                tracing::warn!(server = %server.name, %error, "could not list MCP tools");
                Vec::new()
            }
        }
    }

    /// Every server with its tools, for the management page.
    ///
    /// `connect` decides whether unreachable servers are dialled again. The page
    /// passes `false` on load so opening it is instant, and `true` when the user
    /// asks for a refresh.
    pub async fn list_with_tools(&self, connect: bool) -> Result<Vec<ServerWithTools>> {
        let servers = self.db.list_mcp_servers()?;
        let mut out = Vec::with_capacity(servers.len());

        for server in servers {
            let remote = if connect && server.enabled {
                self.tools_for(&server).await
            } else {
                self.cache
                    .lock()
                    .await
                    .get(&server.id)
                    .map(|cached| cached.tools.clone())
                    .unwrap_or_default()
            };

            // Named exactly as `tool_specs` names them, so the page shows the name
            // the model is actually given rather than the server's internal one.
            let mut taken: Vec<String> = Vec::new();
            let tools = remote
                .iter()
                .map(|tool| {
                    let name = unique_tool_name(&prefixed_name(&server.name, &tool.name), &taken);
                    taken.push(name.clone());
                    ExposedTool {
                        name,
                        remote_name: tool.name.clone(),
                        description: tool.describe(),
                    }
                })
                .collect();

            out.push(ServerWithTools {
                has_auth: server
                    .auth_ref
                    .as_deref()
                    .map(|reference| self.secrets.has(reference))
                    .unwrap_or(false),
                server,
                tools,
            });
        }

        Ok(out)
    }

    /// Tool specs from every enabled server, ready to offer to a model.
    ///
    /// `taken` carries the names already in use — the built-ins go in first — so
    /// an MCP tool can never shadow `web_search`.
    pub async fn tool_specs(&self, taken: &mut Vec<String>) -> Result<Vec<ToolSpec>> {
        let servers: Vec<McpServerRecord> = self
            .db
            .list_mcp_servers()?
            .into_iter()
            .filter(|server| server.enabled && server.is_addressable())
            .collect();

        let mut specs = Vec::new();

        for server in servers {
            for tool in self.tools_for(&server).await {
                let name = unique_tool_name(&prefixed_name(&server.name, &tool.name), taken);
                taken.push(name.clone());

                specs.push(ToolSpec {
                    name,
                    description: tool.describe(),
                    parameters: tool.schema(),
                    origin: ToolOrigin::Mcp {
                        server_id: server.id.clone(),
                        server_name: server.name.clone(),
                        remote_name: tool.name.clone(),
                    },
                });
            }
        }

        Ok(specs)
    }

    /// Run one tool on one server.
    pub async fn call(
        &self,
        server_id: &str,
        remote_name: &str,
        arguments: &Value,
    ) -> Result<(String, bool)> {
        let server = self
            .db
            .get_mcp_server(server_id)?
            .ok_or_else(|| KuroError::not_found(format!("MCP server `{server_id}`")))?;

        if !server.enabled {
            return Err(KuroError::bad_request(format!(
                "`{}` is switched off",
                server.name
            )));
        }

        let bearer = self.bearer_for(&server);
        transport::call_tool(&self.client, &server, bearer.as_deref(), remote_name, arguments).await
    }

    /// Drop a server's cached tools, after its configuration changed.
    pub async fn invalidate(&self, server_id: &str) {
        self.cache.lock().await.remove(server_id);
    }
}

/// Namespace a tool with the server it came from.
///
/// Two servers offering `search` need distinct names, and a model choosing
/// between `github_search` and `exa_search` picks better than one choosing between
/// `search` and `search_2`.
fn prefixed_name(server_name: &str, tool_name: &str) -> String {
    let prefix = sanitize_tool_name(server_name).to_ascii_lowercase();
    let tool = sanitize_tool_name(tool_name);

    // A tool that already names its server does not need it twice.
    if tool.to_ascii_lowercase().starts_with(&prefix) {
        return tool;
    }
    format!("{prefix}_{tool}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewMcpServer;

    fn manager() -> (McpManager, Db) {
        let db = Db::open_in_memory().expect("open");
        let path = std::env::temp_dir().join(format!("kuro-mcp-{}.json", uuid::Uuid::new_v4()));
        let manager = McpManager::new(
            db.clone(),
            SecretStore::new(path),
            reqwest::Client::new(),
        );
        (manager, db)
    }

    #[test]
    fn tools_are_namespaced_by_their_server() {
        assert_eq!(prefixed_name("GitHub", "search_issues"), "github_search_issues");
        assert_eq!(prefixed_name("Exa", "crawl"), "exa_crawl");
    }

    #[test]
    fn a_tool_that_already_names_its_server_is_not_prefixed_twice() {
        assert_eq!(prefixed_name("Exa", "exa_search"), "exa_search");
        assert_eq!(prefixed_name("exa", "Exa_Search"), "Exa_Search");
    }

    #[test]
    fn awkward_server_names_still_produce_valid_tool_names() {
        let name = prefixed_name("My Server (v2)!", "do/thing");
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "got: {name}"
        );
    }

    #[test]
    fn credential_references_are_namespaced_away_from_provider_keys() {
        assert_eq!(auth_reference("abc"), "mcp:abc");
    }

    #[tokio::test]
    async fn a_bearer_token_is_stored_by_reference_and_never_returned_on_the_record() {
        let (manager, db) = manager();
        let server = db
            .insert_mcp_server(
                &NewMcpServer {
                    name: "Exa".to_string(),
                    transport: "http".to_string(),
                    url: Some("https://mcp.exa.ai/mcp".to_string()),
                    ..Default::default()
                },
                None,
            )
            .expect("insert");

        let reference = manager.store_auth(&server.id, "secret-token").expect("store");

        assert_eq!(reference, format!("mcp:{}", server.id));
        let serialised = serde_json::to_string(&server).expect("serialise");
        assert!(
            !serialised.contains("secret-token"),
            "a server record must never carry its token"
        );

        manager.forget_auth(&server).expect("forget");
    }

    #[test]
    fn a_hyphenated_tool_name_survives_prefixing() {
        // MCP servers commonly use hyphens; engines accept them, so they must not
        // be rewritten into underscores.
        assert_eq!(
            prefixed_name("Context7", "resolve-library-id"),
            "context7_resolve-library-id"
        );
    }

    #[tokio::test]
    async fn listing_without_connecting_returns_rows_and_no_tools() {
        let (manager, db) = manager();
        db.insert_mcp_server(
            &NewMcpServer {
                name: "Context7".to_string(),
                transport: "http".to_string(),
                url: Some("https://mcp.context7.com/mcp".to_string()),
                ..Default::default()
            },
            None,
        )
        .expect("insert");

        let listed = manager.list_with_tools(false).await.expect("list");

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].server.name, "Context7");
        assert!(listed[0].tools.is_empty(), "no network call means no tools yet");
        assert!(!listed[0].has_auth);
    }

    #[tokio::test]
    async fn a_disabled_server_offers_no_tools_to_a_model() {
        let (manager, db) = manager();
        let server = db
            .insert_mcp_server(
                &NewMcpServer {
                    name: "Files".to_string(),
                    transport: "stdio".to_string(),
                    command: Some("cat".to_string()),
                    ..Default::default()
                },
                None,
            )
            .expect("insert");
        db.set_mcp_enabled(&server.id, false).expect("disable");

        let mut taken = vec!["web_search".to_string()];
        let specs = manager.tool_specs(&mut taken).await.expect("specs");

        assert!(specs.is_empty());
    }

    #[tokio::test]
    async fn calling_a_disabled_server_is_refused_with_its_name() {
        let (manager, db) = manager();
        let server = db
            .insert_mcp_server(
                &NewMcpServer {
                    name: "Files".to_string(),
                    transport: "stdio".to_string(),
                    command: Some("cat".to_string()),
                    ..Default::default()
                },
                None,
            )
            .expect("insert");
        db.set_mcp_enabled(&server.id, false).expect("disable");

        let error = manager
            .call(&server.id, "read", &serde_json::json!({}))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("Files"), "got: {error}");
        assert!(error.contains("switched off"), "got: {error}");
    }

    #[tokio::test]
    async fn calling_an_unknown_server_is_a_not_found() {
        let (manager, _db) = manager();
        let error = manager.call("nope", "x", &serde_json::json!({})).await.unwrap_err();
        assert!(matches!(error, KuroError::NotFound(_)));
    }

    #[tokio::test]
    async fn a_failed_refresh_is_recorded_on_the_row_with_its_reason() {
        let (manager, db) = manager();
        let server = db
            .insert_mcp_server(
                &NewMcpServer {
                    name: "Missing".to_string(),
                    transport: "stdio".to_string(),
                    command: Some("kuro-not-a-real-binary".to_string()),
                    ..Default::default()
                },
                None,
            )
            .expect("insert");

        assert!(manager.refresh(&server.id).await.is_err());

        let reloaded = db.get_mcp_server(&server.id).expect("get").expect("present");
        assert_eq!(reloaded.status, McpStatus::Error);
        assert!(reloaded.last_error.is_some(), "the reason must be kept for the UI");
    }

    #[tokio::test]
    async fn an_unaddressable_server_is_skipped_when_building_specs() {
        let (manager, db) = manager();
        db.insert_mcp_server(
            &NewMcpServer {
                name: "Incomplete".to_string(),
                transport: "http".to_string(),
                ..Default::default()
            },
            None,
        )
        .expect("insert");

        let mut taken = Vec::new();
        assert!(manager.tool_specs(&mut taken).await.expect("specs").is_empty());
    }
}
