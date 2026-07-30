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

pub mod fetch;
pub mod files;
pub mod html;
pub mod intent;
pub mod memory;
pub mod web_search;

use serde::Serialize;
use serde_json::{json, Value};

use crate::db::Db;
use crate::tools::files::FilePermissions;
use crate::tools::web_search::{SearchConfig, SearchResult};
use crate::Result;

/// Tools that ship with Kuro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    WebSearch,
    FetchUrl,
    Remember,
    Recall,
    ListFiles,
    ReadFile,
    WriteFile,
}

impl Builtin {
    pub const ALL: &'static [Builtin] = &[
        Builtin::WebSearch,
        Builtin::FetchUrl,
        Builtin::Remember,
        Builtin::Recall,
        Builtin::ListFiles,
        Builtin::ReadFile,
        Builtin::WriteFile,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::WebSearch => "web_search",
            Self::FetchUrl => "fetch_url",
            Self::Remember => "remember",
            Self::Recall => "recall",
            Self::ListFiles => "list_files",
            Self::ReadFile => "read_file",
            Self::WriteFile => "write_file",
        }
    }

    /// Whether a call to this tool changes something outside Kuro.
    ///
    /// The interface marks these in the transcript, because "it wrote a file" is
    /// not something a person should have to infer from a tool name.
    pub fn modifies_the_machine(self) -> bool {
        matches!(self, Self::WriteFile)
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
            Self::ListFiles | Self::ReadFile | Self::WriteFile => ToolGroup::Files,
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
            Self::ListFiles => {
                "List the contents of a folder on this computer. Only the folders the user has \
                 granted are reachable. Use this to find a file before reading it."
            }
            Self::ReadFile => {
                "Read a text file from this computer. Only the folders the user has granted are \
                 reachable. Read a file before describing or changing it — never guess at what \
                 it contains."
            }
            Self::WriteFile => {
                "Create or overwrite a text file on this computer, inside the folders the user \
                 has granted. This replaces the whole file. Read it first if you are changing \
                 something that already exists, and say what you wrote afterwards."
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
            Self::ListFiles => json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Folder to list. Absolute, or relative to the first granted folder.",
                    },
                },
                "required": ["path"],
            }),
            Self::ReadFile => json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File to read. Absolute, or relative to the first granted folder.",
                    },
                },
                "required": ["path"],
            }),
            Self::WriteFile => json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File to create or overwrite.",
                    },
                    "content": {
                        "type": "string",
                        "description": "The complete new contents of the file.",
                    },
                },
                "required": ["path", "content"],
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
    /// Reading and writing files on this machine. Gated by [`files::FilePermissions`]
    /// on top of this switch, because the switch alone would be too blunt a
    /// control for something that can change the user's own work.
    Files,
}

impl ToolGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Memory => "memory",
            Self::Files => "files",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "web" => Some(Self::Web),
            "memory" => Some(Self::Memory),
            "files" => Some(Self::Files),
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
    /// Which folders the file tools may touch, and whether they may write.
    pub files: FilePermissions,
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
        Builtin::ListFiles => run_list_files(arguments, context),
        Builtin::ReadFile => run_read_file(arguments, context),
        Builtin::WriteFile => run_write_file(arguments, context),
    }
}

fn run_list_files(arguments: &Value, context: &BuiltinContext<'_>) -> ToolOutcome {
    let Some(path) = string_argument(arguments, "path") else {
        return ToolOutcome::failed("`path` is required and must be a string");
    };

    match context.files.resolve_path(&path, false) {
        Ok(resolved) => match files::list_directory(&resolved) {
            Ok(entries) => ToolOutcome::ok(files::format_listing(&resolved, &entries)),
            Err(error) => ToolOutcome::failed(error),
        },
        Err(error) => ToolOutcome::failed(error),
    }
}

fn run_read_file(arguments: &Value, context: &BuiltinContext<'_>) -> ToolOutcome {
    let Some(path) = string_argument(arguments, "path") else {
        return ToolOutcome::failed("`path` is required and must be a string");
    };

    match context.files.resolve_path(&path, false) {
        Ok(resolved) => match files::read_file(&resolved) {
            Ok(text) => ToolOutcome::ok(format!("`{}`:\n\n{text}", resolved.display())),
            Err(error) => ToolOutcome::failed(error),
        },
        Err(error) => ToolOutcome::failed(error),
    }
}

fn run_write_file(arguments: &Value, context: &BuiltinContext<'_>) -> ToolOutcome {
    let Some(path) = string_argument(arguments, "path") else {
        return ToolOutcome::failed("`path` is required and must be a string");
    };
    // An empty file is a legitimate thing to write, so `content` is read directly
    // rather than through the blank-rejecting helper.
    let Some(content) = arguments.get("content").and_then(Value::as_str) else {
        return ToolOutcome::failed("`content` is required and must be a string");
    };

    match context.files.resolve_path(&path, true) {
        Ok(resolved) => match files::write_file(&resolved, content) {
            Ok(report) => ToolOutcome::ok(report.describe(&resolved)),
            Err(error) => ToolOutcome::failed(error),
        },
        Err(error) => ToolOutcome::failed(error),
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
///
/// File permissions are applied here rather than at call time so that a model is
/// never shown a tool it would only be refused for using. `write_file` in
/// particular is absent entirely at the read-only tier: a model that cannot see
/// it cannot decide to try it, which is a stronger guarantee than refusing the
/// call afterwards and a much clearer one to explain.
pub fn builtins_for_groups(groups: &[ToolGroup], files: &FilePermissions) -> Vec<Builtin> {
    Builtin::ALL
        .iter()
        .copied()
        .filter(|tool| groups.contains(&tool.group()))
        .filter(|tool| {
            if tool.group() != ToolGroup::Files {
                return true;
            }
            files.is_usable() && (!tool.modifies_the_machine() || files.access.allows_write())
        })
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

    /// File permissions with everything granted, for the group tests.
    fn full_file_access() -> FilePermissions {
        FilePermissions {
            access: files::FileAccess::Write,
            roots: vec![std::env::temp_dir()],
        }
    }

    #[test]
    fn groups_bundle_related_tools_behind_one_switch() {
        let none = FilePermissions::default();

        let web = builtins_for_groups(&[ToolGroup::Web], &none);
        assert!(web.contains(&Builtin::WebSearch));
        assert!(web.contains(&Builtin::FetchUrl));
        assert!(!web.contains(&Builtin::Remember));

        let all = builtins_for_groups(
            &[ToolGroup::Web, ToolGroup::Memory, ToolGroup::Files],
            &full_file_access(),
        );
        assert_eq!(all.len(), Builtin::ALL.len());

        assert!(builtins_for_groups(&[], &none).is_empty());
    }

    #[test]
    fn the_file_switch_alone_does_not_grant_file_tools() {
        // Turning the group on without granting a folder must offer nothing,
        // rather than offering tools whose every call is then refused.
        let offered = builtins_for_groups(&[ToolGroup::Files], &FilePermissions::default());
        assert!(offered.is_empty());
    }

    #[test]
    fn the_read_only_tier_does_not_offer_the_write_tool() {
        let read_only = FilePermissions {
            access: files::FileAccess::Read,
            roots: vec![std::env::temp_dir()],
        };

        let offered = builtins_for_groups(&[ToolGroup::Files], &read_only);

        assert!(offered.contains(&Builtin::ReadFile));
        assert!(offered.contains(&Builtin::ListFiles));
        assert!(
            !offered.contains(&Builtin::WriteFile),
            "a model cannot try what it was never shown"
        );
    }

    #[test]
    fn group_names_round_trip() {
        for group in [ToolGroup::Web, ToolGroup::Memory, ToolGroup::Files] {
            assert_eq!(ToolGroup::parse(group.as_str()), Some(group));
        }
        assert_eq!(ToolGroup::parse(" MEMORY "), Some(ToolGroup::Memory));
        assert_eq!(ToolGroup::parse("sql"), None);
    }

    #[test]
    fn only_writing_is_marked_as_changing_the_machine() {
        assert!(Builtin::WriteFile.modifies_the_machine());
        for tool in [Builtin::ReadFile, Builtin::ListFiles, Builtin::WebSearch] {
            assert!(!tool.modifies_the_machine());
        }
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
            files: FilePermissions::default(),
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
            files: FilePermissions::default(),
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
            files: FilePermissions::default(),
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
