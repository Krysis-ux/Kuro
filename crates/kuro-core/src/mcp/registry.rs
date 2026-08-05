//! The recommended MCP servers.
//!
//! Static data, not a fetched catalogue. A local-first app should not have to
//! call home to show a user what they can install, and a list that ships with the
//! binary cannot break because someone else's endpoint moved.
//!
//! Entries are chosen for being useful without a key where possible, and for
//! being run by the people who own the thing they connect to. Anything needing a
//! key says so up front, with a link to where the key comes from — the worst
//! version of this screen is one that offers a server and then fails silently
//! because a token was never set.

use serde::Serialize;

use crate::db::NewMcpServer;

/// What a recommended server needs before it will work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Requirement {
    /// Works immediately.
    None,
    /// Needs a bearer token from the provider.
    ApiKey,
    /// Runs a local command, so it needs Node or another runtime present.
    LocalRuntime,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryEntry {
    pub slug: &'static str,
    pub name: &'static str,
    /// One line, in the same register as the rest of the interface.
    pub blurb: &'static str,
    /// What the tools actually let a model do, for the expanded view.
    pub detail: &'static str,
    pub transport: &'static str,
    pub url: Option<&'static str>,
    pub command: Option<&'static str>,
    pub args: &'static [&'static str],
    pub requirement: Requirement,
    /// Where to get the key, when one is needed.
    pub credentials_url: Option<&'static str>,
    pub homepage: &'static str,
    /// Suggested as a default for a new install.
    pub recommended: bool,
}

impl RegistryEntry {
    /// The insert payload for this entry, given a token the user supplied.
    pub fn to_new_server(&self, auth_token: Option<String>) -> NewMcpServer {
        NewMcpServer {
            name: self.name.to_string(),
            transport: self.transport.to_string(),
            command: self.command.map(str::to_string),
            args: self.args.iter().map(|arg| arg.to_string()).collect(),
            env: Default::default(),
            url: self.url.map(str::to_string),
            headers: Default::default(),
            slug: Some(self.slug.to_string()),
            auth_token,
        }
    }
}

/// The catalogue.
pub const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        slug: "context7",
        name: "Context7",
        blurb: "Up-to-date documentation for libraries and frameworks.",
        detail: "Resolves a library name to its current docs and pulls the relevant pages, \
                 so a model answers from this year's API rather than its training data.",
        transport: "http",
        url: Some("https://mcp.context7.com/mcp"),
        command: None,
        args: &[],
        requirement: Requirement::None,
        credentials_url: None,
        homepage: "https://context7.com",
        recommended: true,
    },
    RegistryEntry {
        slug: "deepwiki",
        name: "DeepWiki",
        blurb: "Ask questions about any public GitHub repository.",
        detail: "Indexes a repository and answers questions about its structure and behaviour \
                 without cloning it first.",
        transport: "http",
        url: Some("https://mcp.deepwiki.com/mcp"),
        command: None,
        args: &[],
        requirement: Requirement::None,
        credentials_url: None,
        homepage: "https://deepwiki.com",
        recommended: true,
    },
    RegistryEntry {
        slug: "exa",
        name: "Exa",
        blurb: "Search the web and read full pages as clean markdown.",
        detail: "A search API built for models rather than for people: results come back as \
                 readable text instead of a page of markup. An alternative to Kuro's built-in \
                 search when you want higher quality.",
        transport: "http",
        url: Some("https://mcp.exa.ai/mcp"),
        command: None,
        args: &[],
        requirement: Requirement::ApiKey,
        credentials_url: Some("https://dashboard.exa.ai/api-keys"),
        homepage: "https://exa.ai",
        recommended: false,
    },
    RegistryEntry {
        slug: "github",
        name: "GitHub",
        blurb: "Repositories, issues, pull requests and code search.",
        detail: "Read and act on GitHub through your own account. Needs a personal access token; \
                 the scopes you grant it are exactly what the model can do.",
        transport: "http",
        url: Some("https://api.githubcopilot.com/mcp/"),
        command: None,
        args: &[],
        requirement: Requirement::ApiKey,
        credentials_url: Some("https://github.com/settings/personal-access-tokens"),
        homepage: "https://github.com/github/github-mcp-server",
        recommended: false,
    },
    RegistryEntry {
        slug: "huggingface",
        name: "Hugging Face",
        blurb: "Search models, datasets, spaces and papers.",
        detail: "Useful next to Kuro's own model list: find a GGUF repository by describing what \
                 you want, then paste it into Models.",
        transport: "http",
        url: Some("https://huggingface.co/mcp"),
        command: None,
        args: &[],
        requirement: Requirement::None,
        credentials_url: Some("https://huggingface.co/settings/tokens"),
        homepage: "https://huggingface.co/settings/mcp",
        recommended: false,
    },
    RegistryEntry {
        slug: "filesystem",
        name: "Filesystem",
        blurb: "Read and write files in folders you choose.",
        detail: "Runs on this machine. It can only see the directories named in its arguments, \
                 so add the folder you want the model to work in and nothing else.",
        transport: "stdio",
        url: None,
        command: Some("npx"),
        args: &["-y", "@modelcontextprotocol/server-filesystem"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/modelcontextprotocol/servers",
        recommended: false,
    },
    RegistryEntry {
        slug: "fetch",
        name: "Fetch",
        blurb: "Retrieve a URL and convert it to markdown.",
        detail: "A local alternative to Kuro's built-in page reader, with its own conversion rules.",
        transport: "stdio",
        url: None,
        command: Some("uvx"),
        args: &["mcp-server-fetch"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/modelcontextprotocol/servers",
        recommended: false,
    },
    RegistryEntry {
        slug: "sequential-thinking",
        name: "Sequential Thinking",
        blurb: "A scratchpad for working through a problem in steps.",
        detail: "Gives a model somewhere to plan, revise and branch before answering. Most useful \
                 with smaller models, which benefit from being made to slow down.",
        transport: "stdio",
        url: None,
        command: Some("npx"),
        args: &["-y", "@modelcontextprotocol/server-sequential-thinking"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/modelcontextprotocol/servers",
        recommended: false,
    },
    RegistryEntry {
        slug: "playwright",
        name: "Playwright",
        blurb: "Drive a real browser: open pages, click things, take screenshots.",
        detail: "Runs a browser on this machine and lets a model navigate it — filling forms, \
                 clicking through a flow, reading what a page actually renders. This is how a \
                 model checks a web page rather than assuming what it looks like. It downloads \
                 a browser on first run.",
        transport: "stdio",
        url: None,
        command: Some("npx"),
        args: &["-y", "@playwright/mcp@latest"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/microsoft/playwright-mcp",
        recommended: false,
    },
    RegistryEntry {
        slug: "chrome-devtools",
        name: "Chrome DevTools",
        blurb: "Inspect a running page: console errors, network requests, performance.",
        detail: "Connects to Chrome and exposes what the developer tools show — the console, \
                 failed requests, and performance traces. Pairs with a dev server: start the \
                 server on the Code page, then ask why the page is throwing.",
        transport: "stdio",
        url: None,
        command: Some("npx"),
        args: &["-y", "chrome-devtools-mcp@latest"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/ChromeDevTools/chrome-devtools-mcp",
        recommended: false,
    },
    RegistryEntry {
        slug: "git",
        name: "Git",
        blurb: "Read history, diffs and blame in a local repository.",
        detail: "Answers 'when did this change and why' from the repository itself, rather than \
                 from the current state of the files. Add the repository path as an argument.",
        transport: "stdio",
        url: None,
        command: Some("uvx"),
        args: &["mcp-server-git"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/modelcontextprotocol/servers",
        recommended: false,
    },
    RegistryEntry {
        slug: "postgres",
        name: "PostgreSQL",
        blurb: "Inspect a database's schema and run read-only queries.",
        detail: "Read-only. Lets a model see the real table definitions before writing a query \
                 against them, which is the difference between a migration that applies and one \
                 that references a column nobody has. The connection string goes in the arguments.",
        transport: "stdio",
        url: None,
        command: Some("npx"),
        args: &["-y", "@modelcontextprotocol/server-postgres"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/modelcontextprotocol/servers",
        recommended: false,
    },
    RegistryEntry {
        slug: "sqlite",
        name: "SQLite",
        blurb: "Query and inspect a SQLite database file.",
        detail: "Points at a `.db` file on this machine and exposes its tables. Useful for a \
                 project that keeps its data in one, which is most small ones.",
        transport: "stdio",
        url: None,
        command: Some("uvx"),
        args: &["mcp-server-sqlite"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/modelcontextprotocol/servers",
        recommended: false,
    },
    RegistryEntry {
        slug: "memory",
        name: "Knowledge Graph Memory",
        blurb: "A structured memory of entities and how they relate.",
        detail: "Different from Kuro's own memory, which stores sentences. This stores a graph — \
                 people, projects, and the relationships between them — which answers 'what do \
                 you know about X' more precisely at the cost of more setup.",
        transport: "stdio",
        url: None,
        command: Some("npx"),
        args: &["-y", "@modelcontextprotocol/server-memory"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/modelcontextprotocol/servers",
        recommended: false,
    },
    RegistryEntry {
        slug: "time",
        name: "Time",
        blurb: "The current date and time, in any timezone.",
        detail: "Small and worth more than it looks: a model has no clock, and will otherwise \
                 answer 'what is today's date' from its training data and be months wrong.",
        transport: "stdio",
        url: None,
        command: Some("uvx"),
        args: &["mcp-server-time"],
        requirement: Requirement::LocalRuntime,
        credentials_url: None,
        homepage: "https://github.com/modelcontextprotocol/servers",
        recommended: false,
    },
    RegistryEntry {
        slug: "linear",
        name: "Linear",
        blurb: "Read and update issues in Linear.",
        detail: "Run by Linear. Lets a model look up the issue being worked on, read its \
                 description and comments, and update it when the work is done.",
        transport: "http",
        url: Some("https://mcp.linear.app/mcp"),
        command: None,
        args: &[],
        requirement: Requirement::None,
        credentials_url: None,
        homepage: "https://linear.app/docs/mcp",
        recommended: false,
    },
    RegistryEntry {
        slug: "sentry",
        name: "Sentry",
        blurb: "Look up an error and its stack trace.",
        detail: "Run by Sentry. Turns 'the app is crashing' into the actual exception, the \
                 release it started in, and how many people it has hit.",
        transport: "http",
        url: Some("https://mcp.sentry.dev/mcp"),
        command: None,
        args: &[],
        requirement: Requirement::None,
        credentials_url: None,
        homepage: "https://docs.sentry.io/product/sentry-mcp/",
        recommended: false,
    },
    RegistryEntry {
        slug: "notion",
        name: "Notion",
        blurb: "Search and read pages in a Notion workspace.",
        detail: "Run by Notion. Useful when the specification the code is meant to implement \
                 lives in a document rather than in the repository.",
        transport: "http",
        url: Some("https://mcp.notion.com/mcp"),
        command: None,
        args: &[],
        requirement: Requirement::None,
        credentials_url: None,
        homepage: "https://developers.notion.com/docs/mcp",
        recommended: false,
    },
];

pub fn find(slug: &str) -> Option<&'static RegistryEntry> {
    ENTRIES.iter().find(|entry| entry.slug == slug)
}

/// The entries offered first on a fresh install.
pub fn recommended() -> Vec<&'static RegistryEntry> {
    ENTRIES.iter().filter(|entry| entry.recommended).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slug_is_unique_and_findable() {
        let mut seen: Vec<&str> = Vec::new();
        for entry in ENTRIES {
            assert!(!seen.contains(&entry.slug), "duplicate slug `{}`", entry.slug);
            seen.push(entry.slug);
            assert!(find(entry.slug).is_some());
        }
        assert!(find("nonexistent").is_none());
    }

    #[test]
    fn every_entry_is_addressable_by_its_declared_transport() {
        for entry in ENTRIES {
            match entry.transport {
                "http" => assert!(
                    entry.url.is_some_and(|url| url.starts_with("https://")),
                    "`{}` is http but has no https URL",
                    entry.slug
                ),
                "stdio" => assert!(
                    entry.command.is_some(),
                    "`{}` is stdio but has no command",
                    entry.slug
                ),
                other => panic!("`{}` has an unknown transport `{other}`", entry.slug),
            }
        }
    }

    #[test]
    fn anything_needing_a_key_says_where_to_get_one() {
        for entry in ENTRIES.iter().filter(|e| e.requirement == Requirement::ApiKey) {
            assert!(
                entry.credentials_url.is_some(),
                "`{}` requires a key but does not say where from",
                entry.slug
            );
        }
    }

    #[test]
    fn every_entry_is_described_in_two_registers() {
        for entry in ENTRIES {
            assert!(!entry.blurb.is_empty(), "`{}` has no blurb", entry.slug);
            assert!(
                entry.detail.len() > entry.blurb.len(),
                "`{}`'s detail should say more than its blurb",
                entry.slug
            );
            assert!(entry.homepage.starts_with("https://"), "`{}`", entry.slug);
        }
    }

    #[test]
    fn the_default_offer_needs_no_setup() {
        let defaults = recommended();
        assert!(!defaults.is_empty(), "a fresh install should have something to suggest");

        for entry in defaults {
            assert_eq!(
                entry.requirement,
                Requirement::None,
                "`{}` is offered by default but needs setup",
                entry.slug
            );
        }
    }

    #[test]
    fn an_entry_converts_into_an_insertable_server() {
        let entry = find("filesystem").expect("filesystem");
        let new_server = entry.to_new_server(None);

        assert_eq!(new_server.name, "Filesystem");
        assert_eq!(new_server.transport, "stdio");
        assert_eq!(new_server.command.as_deref(), Some("npx"));
        assert_eq!(new_server.args.len(), 2);
        assert_eq!(new_server.slug.as_deref(), Some("filesystem"));
        assert_eq!(new_server.auth_token, None);
    }

    #[test]
    fn a_token_supplied_by_the_user_is_carried_through() {
        let entry = find("exa").expect("exa");
        let new_server = entry.to_new_server(Some("secret".to_string()));
        assert_eq!(new_server.auth_token.as_deref(), Some("secret"));
    }
}
