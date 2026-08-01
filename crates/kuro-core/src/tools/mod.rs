//! What a model can do besides produce text.
//!
//! Two kinds of tool meet here and are deliberately made indistinguishable to
//! everything downstream:
//!
//! * **Built-ins** — web search, page fetch, memory. They ship with Kuro, need
//!   no setup, and are the reason a fresh install can already look things up.
//! * **MCP tools** — whatever the servers the user connected happen to expose.
//!
//! Both become a [`ToolSpec`], both are offered to the model in the same
//! OpenAI-shaped `tools` array, and both return a [`ToolOutcome`]. The chat loop
//! never asks where a tool came from.
//!
//! ## Nothing here touches the filesystem
//!
//! There are deliberately no file tools in this set. Reading and writing files
//! lives in [`crate::workspace`], behind a coding workspace: a folder the user
//! picked and a mode saying what may happen inside it. A chat has no folder, so
//! it has nothing to scope file access to, and a switch that granted it would be
//! granting the whole machine.
//!
//! The path helpers in [`files`] are still used — by the workspace tools, which
//! is now their only caller.

pub mod fetch;
pub mod files;
pub mod html;
pub mod intent;
pub mod memory;
pub mod projects;
pub mod web_search;

use serde::Serialize;
use serde_json::{json, Value};

use crate::db::Db;
use crate::tools::web_search::{SearchConfig, SearchResult};
use crate::Result;

/// Tools that ship with Kuro.
///
/// Every one is read-only with respect to this machine: they reach the network,
/// Kuro's own database, or — through [`projects`] — the *contents* of a folder
/// the user already chose on the Code page. None of them writes a file, and the
/// three project tools cannot, structurally rather than by convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    WebSearch,
    FetchUrl,
    Remember,
    Recall,
    ListProjects,
    ReadProjectFile,
    SearchProjects,
}

impl Builtin {
    pub const ALL: &'static [Builtin] = &[
        Builtin::WebSearch,
        Builtin::FetchUrl,
        Builtin::Remember,
        Builtin::Recall,
        Builtin::ListProjects,
        Builtin::ReadProjectFile,
        Builtin::SearchProjects,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::WebSearch => "web_search",
            Self::FetchUrl => "fetch_url",
            Self::Remember => "remember",
            Self::Recall => "recall",
            Self::ListProjects => "list_projects",
            Self::ReadProjectFile => "read_project_file",
            Self::SearchProjects => "search_projects",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|tool| tool.name() == name)
    }

    /// Which switch in the composer turns this tool on.
    ///
    /// Grouping them means a user enables "web" or "memory" rather than four
    /// individual functions, while the model still sees separate tools because
    /// that is what it is good at choosing between.
    pub fn group(self) -> ToolGroup {
        match self {
            Self::WebSearch | Self::FetchUrl => ToolGroup::Web,
            Self::Remember | Self::Recall => ToolGroup::Memory,
            Self::ListProjects | Self::ReadProjectFile | Self::SearchProjects => {
                ToolGroup::Projects
            }
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::WebSearch => {
                "Search the web. Use this for anything current, anything after your training \
                 cutoff, or any fact you are not certain of. Returns titles, URLs and snippets."
            }
            Self::FetchUrl => {
                "Read the text of a web page. Use this after web_search when a snippet is not \
                 enough, or when the user gives you a URL."
            }
            Self::Remember => {
                "Save a durable fact about the user or their work so it is available in later \
                 conversations. Use it for stable preferences and decisions, not for passing detail."
            }
            Self::Recall => {
                "Look up facts saved earlier with remember. Use this when the user refers to \
                 something they told you before."
            }
            Self::ListProjects => {
                "List the user's coding workspaces — the folders they opened on the Code \
                 page. Call this first when they mention their project, their app, or a \
                 codebase, so you know which ones exist and what they are called."
            }
            Self::ReadProjectFile => {
                "Read a file out of one of the user's coding workspaces. This is read-only: \
                 you can quote it, explain it and propose changes, and you cannot edit it. \
                 Say so plainly if they ask you to make the change."
            }
            Self::SearchProjects => {
                "Search inside one coding workspace for a literal string, or leave the query \
                 out to see its file layout. Use this to find where something lives before \
                 reading it."
            }
        }
    }

    fn parameters(self) -> Value {
        match self {
            Self::WebSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to search for." },
                    "max_results": {
                        "type": "integer",
                        "description": "How many results to return (1-10).",
                        "minimum": 1,
                        "maximum": 10,
                    },
                },
                "required": ["query"],
            }),
            Self::FetchUrl => json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "An http or https URL. Public addresses only.",
                    },
                },
                "required": ["url"],
            }),
            Self::Remember => json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The fact to remember, written as a standalone sentence.",
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional labels to make the fact easier to find later.",
                    },
                },
                "required": ["content"],
            }),
            Self::Recall => json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Words to look for. Leave empty for the most recent facts.",
                    },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 25 },
                },
            }),
            Self::ListProjects => json!({
                "type": "object",
                "properties": {},
            }),
            Self::ReadProjectFile => json!({
                "type": "object",
                "properties": {
                    "project": {
                        "type": "string",
                        "description": "The workspace name, as list_projects gave it.",
                    },
                    "path": {
                        "type": "string",
                        "description": "Path relative to that project's folder, such as src/main.rs.",
                    },
                },
                "required": ["project", "path"],
            }),
            Self::SearchProjects => json!({
                "type": "object",
                "properties": {
                    "project": {
                        "type": "string",
                        "description": "The workspace name, as list_projects gave it.",
                    },
                    "query": {
                        "type": "string",
                        "description":
                            "The exact text to look for. Not a regular expression. Leave it \
                             out to see the project's file layout instead.",
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Match case exactly. Defaults to false.",
                    },
                },
                "required": ["project"],
            }),
        }
    }

    pub fn spec(self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
            origin: ToolOrigin::Builtin,
        }
    }
}

/// The switch a user flips, rather than the function a model calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolGroup {
    Web,
    Memory,
    /// Reading the user's coding workspaces. Read-only, always — see
    /// [`projects`] for why that is structural rather than a promise.
    Projects,
}

impl ToolGroup {
    pub const ALL: &'static [ToolGroup] = &[Self::Web, Self::Memory, Self::Projects];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Memory => "memory",
            Self::Projects => "projects",
        }
    }

    /// The words next to the switch.
    pub fn label(self) -> &'static str {
        match self {
            Self::Web => "Web",
            Self::Memory => "Memory",
            Self::Projects => "Projects",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Self::Web => "Search the web before answering. Queries leave this machine.",
            Self::Memory => "Read and save durable facts. Never leaves this machine.",
            Self::Projects => {
                "Read the folders you opened on the Code page. Reading only — chat can never \
                 change a file."
            }
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "web" => Some(Self::Web),
            "memory" => Some(Self::Memory),
            "projects" | "project" | "code" => Some(Self::Projects),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ToolOrigin {
    Builtin,
    /// A tool borrowed from a connected MCP server. `remote_name` is the name the
    /// server itself uses, which is not necessarily what the model is shown.
    Mcp {
        server_id: String,
        server_name: String,
        remote_name: String,
    },
}

/// One callable tool, whatever its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolSpec {
    /// The name the model calls. Unique across every source.
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments.
    pub parameters: Value,
    pub origin: ToolOrigin,
}

impl ToolSpec {
    /// The `tools` array entry an OpenAI-compatible engine expects.
    pub fn to_openai(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            },
        })
    }
}

/// The result of running one tool.
#[derive(Debug, Clone, Default)]
pub struct ToolOutcome {
    /// What the model is shown. On failure this is the reason, because a model
    /// that is told why a call failed can often recover by trying differently.
    pub content: String,
    pub is_error: bool,
    /// Pages this call surfaced, collected so the UI can cite them under the
    /// reply rather than relying on the model to repeat the URLs.
    pub sources: Vec<WebSource>,
}

impl ToolOutcome {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
            sources: Vec::new(),
        }
    }

    /// A failure the model is told about rather than one that ends the turn.
    pub fn failed(reason: impl std::fmt::Display) -> Self {
        Self {
            content: format!("Tool call failed: {reason}"),
            is_error: true,
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebSource {
    pub title: String,
    pub url: String,
}

impl From<&SearchResult> for WebSource {
    fn from(result: &SearchResult) -> Self {
        Self {
            title: result.title.clone(),
            url: result.url.clone(),
        }
    }
}

/// What a built-in needs in order to run.
pub struct BuiltinContext<'a> {
    pub db: &'a Db,
    pub client: &'a reqwest::Client,
    pub search: SearchConfig,
    /// Recorded on anything the model remembers, so a fact can be traced back to
    /// the conversation that produced it.
    pub conversation_id: Option<&'a str>,
}

/// Run one built-in tool.
///
/// Errors are returned as an error-flagged [`ToolOutcome`] rather than as a
/// `Result`, because a tool failing is a normal part of a turn: the model is told
/// what went wrong and gets to try something else. Only a programming mistake
/// would justify ending the generation.
pub async fn run_builtin(tool: Builtin, arguments: &Value, context: &BuiltinContext<'_>) -> ToolOutcome {
    match tool {
        Builtin::WebSearch => run_web_search(arguments, context).await,
        Builtin::FetchUrl => run_fetch(arguments, context).await,
        Builtin::Remember => run_remember(arguments, context),
        Builtin::Recall => run_recall(arguments, context),
        Builtin::ListProjects => projects::list(context.db),
        Builtin::ReadProjectFile => projects::read_file(context.db, arguments),
        Builtin::SearchProjects => projects::search_project(context.db, arguments),
    }
}

async fn run_web_search(arguments: &Value, context: &BuiltinContext<'_>) -> ToolOutcome {
    let Some(query) = string_argument(arguments, "query") else {
        return ToolOutcome::failed("`query` is required and must be a string");
    };

    let max_results = arguments
        .get("max_results")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(web_search::DEFAULT_MAX_RESULTS);

    match web_search::search(context.client, &context.search, &query, max_results).await {
        Ok(results) => ToolOutcome {
            content: web_search::format_for_model(&query, &results),
            is_error: false,
            sources: results.iter().map(WebSource::from).collect(),
        },
        Err(error) => ToolOutcome::failed(error),
    }
}

async fn run_fetch(arguments: &Value, context: &BuiltinContext<'_>) -> ToolOutcome {
    let Some(url) = string_argument(arguments, "url") else {
        return ToolOutcome::failed("`url` is required and must be a string");
    };

    match fetch::fetch_url(context.client, &url).await {
        Ok(page) => ToolOutcome {
            content: fetch::format_for_model(&page),
            is_error: false,
            sources: vec![WebSource {
                title: page.title.clone().unwrap_or_else(|| page.url.clone()),
                url: page.url.clone(),
            }],
        },
        Err(error) => ToolOutcome::failed(error),
    }
}

fn run_remember(arguments: &Value, context: &BuiltinContext<'_>) -> ToolOutcome {
    let Some(content) = string_argument(arguments, "content") else {
        return ToolOutcome::failed("`content` is required and must be a string");
    };

    let tags: Vec<String> = arguments
        .get("tags")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    match context.db.remember(&content, &tags, context.conversation_id) {
        Ok(stored) => ToolOutcome::ok(format!("Remembered: {}", stored.content)),
        Err(error) => ToolOutcome::failed(error),
    }
}

fn run_recall(arguments: &Value, context: &BuiltinContext<'_>) -> ToolOutcome {
    let query = string_argument(arguments, "query").unwrap_or_default();
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| (value as usize).clamp(1, 25))
        .unwrap_or(10);

    match context.db.recall_memories(&query, limit) {
        Ok(found) => ToolOutcome::ok(memory::format_for_model(&query, &found)),
        Err(error) => ToolOutcome::failed(error),
    }
}

/// Read a string argument, tolerating the whitespace models like to add.
fn string_argument(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Arguments as the engine sent them.
///
/// The OpenAI shape puts arguments in a JSON *string*, and models sometimes send
/// an object instead, or a string that is not valid JSON at all. All three are
/// handled here so the tool layer only ever sees an object.
pub fn parse_arguments(raw: Option<&Value>) -> Value {
    match raw {
        Some(Value::String(text)) => serde_json::from_str(text).unwrap_or_else(|_| json!({})),
        Some(Value::Object(map)) => Value::Object(map.clone()),
        _ => json!({}),
    }
}

/// Make a name safe to expose to a model.
///
/// Engines validate tool names against `^[a-zA-Z0-9_-]+$`, and MCP servers are
/// free to use anything. A rejected name would fail the whole request rather than
/// just that tool, so names are rewritten rather than filtered out.
pub fn sanitize_tool_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();

    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "tool".to_string()
    } else {
        trimmed.chars().take(60).collect()
    }
}

/// Give a tool a name no other tool is using.
///
/// Two MCP servers offering `search` would otherwise collide, and whichever the
/// engine picked would be arbitrary.
pub fn unique_tool_name(preferred: &str, taken: &[String]) -> String {
    let base = sanitize_tool_name(preferred);
    if !taken.iter().any(|name| name == &base) {
        return base;
    }

    for suffix in 2..100 {
        let candidate = format!("{base}_{suffix}");
        if !taken.iter().any(|name| name == &candidate) {
            return candidate;
        }
    }

    format!("{base}_{}", uuid::Uuid::new_v4().simple())
}

/// Which built-ins are on, given the groups the user enabled.
pub fn builtins_for_groups(groups: &[ToolGroup]) -> Vec<Builtin> {
    Builtin::ALL
        .iter()
        .copied()
        .filter(|tool| groups.contains(&tool.group()))
        .collect()
}

/// Everything the settings layer needs to describe a built-in to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct BuiltinDescription {
    pub name: String,
    pub description: String,
    pub group: ToolGroup,
}

pub fn describe_builtins() -> Vec<BuiltinDescription> {
    Builtin::ALL
        .iter()
        .map(|tool| BuiltinDescription {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            group: tool.group(),
        })
        .collect()
}

/// Sanity check that no two built-ins share a name, which would make dispatch
/// ambiguous. Called once at startup so a bad edit fails loudly.
pub fn assert_builtin_names_are_unique() -> Result<()> {
    let mut seen: Vec<&str> = Vec::new();
    for tool in Builtin::ALL {
        if seen.contains(&tool.name()) {
            return Err(crate::KuroError::other(format!(
                "two built-in tools are both called `{}`",
                tool.name()
            )));
        }
        seen.push(tool.name());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_has_a_unique_name_that_parses_back() {
        assert_builtin_names_are_unique().expect("names must be unique");
        for tool in Builtin::ALL {
            assert_eq!(Builtin::parse(tool.name()), Some(*tool));
        }
        assert_eq!(Builtin::parse("not_a_tool"), None);
    }

    #[test]
    fn the_openai_shape_carries_name_description_and_schema() {
        let spec = Builtin::WebSearch.spec();
        let encoded = spec.to_openai();

        assert_eq!(encoded["type"], "function");
        assert_eq!(encoded["function"]["name"], "web_search");
        assert_eq!(encoded["function"]["parameters"]["required"][0], "query");
        assert!(encoded["function"]["description"].as_str().unwrap().len() > 20);
    }

    #[test]
    fn groups_bundle_related_tools_behind_one_switch() {
        let web = builtins_for_groups(&[ToolGroup::Web]);
        assert!(web.contains(&Builtin::WebSearch));
        assert!(web.contains(&Builtin::FetchUrl));
        assert!(!web.contains(&Builtin::Remember));

        let projects = builtins_for_groups(&[ToolGroup::Projects]);
        assert!(projects.contains(&Builtin::ReadProjectFile));
        assert!(
            !projects.contains(&Builtin::WebSearch),
            "reading a project does not imply reaching the network"
        );

        let all = builtins_for_groups(ToolGroup::ALL);
        assert_eq!(all.len(), Builtin::ALL.len());

        assert!(builtins_for_groups(&[]).is_empty());
    }

    #[test]
    fn no_builtin_can_change_a_file() {
        // The guarantee the chat surface rests on, and it is narrower than it
        // used to be. A chat can now *read* a folder the user opened on the Code
        // page, because refusing to answer a question about code the user is
        // plainly looking at helped nobody. It still cannot write one: there is
        // no builtin that takes file contents, and the read tools construct
        // their permissions at Plan tier regardless of the workspace's own mode
        // — see `tools::projects`.
        for tool in Builtin::ALL {
            let name = tool.name();
            assert!(
                !name.starts_with("write") && !name.starts_with("edit") && !name.contains("delete"),
                "`{name}` sounds like it changes a file, which chat must never do"
            );
            let takes_contents = tool.spec().parameters["properties"]
                .as_object()
                .is_some_and(|properties| properties.contains_key("content"));
            assert!(
                !takes_contents || *tool == Builtin::Remember,
                "`{name}` accepts contents, which only makes sense for a tool that writes"
            );
        }

        // The workspace's own write tools are not reachable through this enum,
        // so a chat naming one gets "no such tool" rather than a write.
        assert_eq!(Builtin::parse("write_file"), None);
        assert_eq!(Builtin::parse("edit_file"), None);
        assert_eq!(Builtin::parse("run_command"), None);
    }

    #[test]
    fn group_names_round_trip() {
        for group in ToolGroup::ALL {
            assert_eq!(ToolGroup::parse(group.as_str()), Some(*group));
        }
        assert_eq!(ToolGroup::parse(" MEMORY "), Some(ToolGroup::Memory));
        assert_eq!(ToolGroup::parse("sql"), None);
        assert_eq!(
            ToolGroup::parse("files"),
            None,
            "the files switch is gone; a stored setting naming it must not resurrect it"
        );
    }

    #[test]
    fn arguments_are_read_from_a_json_string_or_an_object() {
        let from_string = parse_arguments(Some(&json!(r#"{"query":"rust"}"#)));
        assert_eq!(from_string["query"], "rust");

        let from_object = parse_arguments(Some(&json!({ "query": "rust" })));
        assert_eq!(from_object["query"], "rust");
    }

    #[test]
    fn unreadable_arguments_become_an_empty_object_not_a_panic() {
        assert_eq!(parse_arguments(Some(&json!("not json at all"))), json!({}));
        assert_eq!(parse_arguments(Some(&Value::Null)), json!({}));
        assert_eq!(parse_arguments(None), json!({}));
    }

    #[test]
    fn string_arguments_are_trimmed_and_blanks_treated_as_absent() {
        let arguments = json!({ "query": "  rust  ", "empty": "   " });
        assert_eq!(string_argument(&arguments, "query").as_deref(), Some("rust"));
        assert_eq!(string_argument(&arguments, "empty"), None);
        assert_eq!(string_argument(&arguments, "missing"), None);
    }

    #[test]
    fn tool_names_are_rewritten_to_what_engines_accept() {
        assert_eq!(sanitize_tool_name("search"), "search");
        assert_eq!(sanitize_tool_name("brave/web.search"), "brave_web_search");
        assert_eq!(sanitize_tool_name("  spaced name  "), "spaced_name");
        assert_eq!(sanitize_tool_name("!!!"), "tool", "a name of only punctuation still needs to exist");
    }

    #[test]
    fn long_names_are_shortened_rather_than_rejected() {
        assert_eq!(sanitize_tool_name(&"a".repeat(200)).len(), 60);
    }

    #[test]
    fn colliding_tool_names_are_made_distinct() {
        let taken = vec!["search".to_string()];
        assert_eq!(unique_tool_name("search", &taken), "search_2");

        let taken = vec!["search".to_string(), "search_2".to_string()];
        assert_eq!(unique_tool_name("search", &taken), "search_3");

        assert_eq!(unique_tool_name("search", &[]), "search");
    }

    #[test]
    fn a_failed_outcome_explains_itself_to_the_model() {
        let outcome = ToolOutcome::failed("the URL has no host");
        assert!(outcome.is_error);
        assert!(outcome.content.contains("no host"));
        assert!(outcome.sources.is_empty());
    }

    #[test]
    fn describing_builtins_covers_all_of_them() {
        assert_eq!(describe_builtins().len(), Builtin::ALL.len());
    }

    #[tokio::test]
    async fn a_builtin_called_without_its_required_argument_reports_that() {
        let db = Db::open_in_memory().expect("open");
        let client = reqwest::Client::new();
        let context = BuiltinContext {
            db: &db,
            client: &client,
            search: SearchConfig::default(),
            conversation_id: None,
        };

        let outcome = run_builtin(Builtin::WebSearch, &json!({}), &context).await;

        assert!(outcome.is_error);
        assert!(outcome.content.contains("query"), "got: {}", outcome.content);
    }

    #[tokio::test]
    async fn memory_tools_round_trip_through_the_database() {
        let db = Db::open_in_memory().expect("open");
        let client = reqwest::Client::new();
        let context = BuiltinContext {
            db: &db,
            client: &client,
            search: SearchConfig::default(),
            conversation_id: Some("conversation-1"),
        };

        let stored = run_builtin(
            Builtin::Remember,
            &json!({ "content": "the API base URL is example.com", "tags": ["api"] }),
            &context,
        )
        .await;
        assert!(!stored.is_error, "{}", stored.content);

        let recalled = run_builtin(Builtin::Recall, &json!({ "query": "API base" }), &context).await;

        assert!(!recalled.is_error);
        assert!(recalled.content.contains("example.com"), "got: {}", recalled.content);
    }

    #[tokio::test]
    async fn fetch_refuses_a_loopback_url_chosen_by_the_model() {
        let db = Db::open_in_memory().expect("open");
        let client = reqwest::Client::new();
        let context = BuiltinContext {
            db: &db,
            client: &client,
            search: SearchConfig::default(),
            conversation_id: None,
        };

        let outcome = run_builtin(
            Builtin::FetchUrl,
            &json!({ "url": "http://127.0.0.1:8420/api/settings" }),
            &context,
        )
        .await;

        assert!(outcome.is_error, "a tool must not be able to read Kuro's own API");
    }
}
