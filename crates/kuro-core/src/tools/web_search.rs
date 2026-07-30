//! Web search.
//!
//! The point of this module is that search works the moment Kuro is installed.
//! Requiring an API key before a model can look something up would make the
//! feature theoretical for most people, so the default provider is DuckDuckGo's
//! keyless HTML endpoint. Scraping is admittedly fragile — DuckDuckGo can change
//! its markup — so a failure there is reported as a provider problem with a
//! pointer to the alternatives, never as "nothing was found".
//!
//! Everything else is a proper API and needs a key: Brave and Tavily are hosted,
//! and SearXNG is whatever instance the user runs. All four return the same
//! [`SearchResult`] shape, so the tool layer and the UI never branch on provider.

use serde::{Deserialize, Serialize};

use crate::tools::html;
use crate::{KuroError, Result};

/// Results asked of a provider when the caller has no preference. Enough to
/// answer a question, few enough not to swamp a small context window.
pub const DEFAULT_MAX_RESULTS: usize = 5;
/// Ceiling regardless of what a model asks for.
const RESULT_LIMIT: usize = 10;
/// Snippets are truncated to this many characters. A provider occasionally
/// returns most of a page, which would crowd out the rest of the results.
const MAX_SNIPPET_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchProvider {
    /// Keyless. The default, so search works with no setup.
    Duckduckgo,
    Brave,
    Tavily,
    /// A SearXNG instance, self-hosted or otherwise, addressed by URL.
    Searxng,
}

impl SearchProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Duckduckgo => "duckduckgo",
            Self::Brave => "brave",
            Self::Tavily => "tavily",
            Self::Searxng => "searxng",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "duckduckgo" | "ddg" | "" => Some(Self::Duckduckgo),
            "brave" => Some(Self::Brave),
            "tavily" => Some(Self::Tavily),
            "searxng" | "searx" => Some(Self::Searxng),
            _ => None,
        }
    }

    pub fn needs_api_key(self) -> bool {
        matches!(self, Self::Brave | Self::Tavily)
    }

    pub fn needs_base_url(self) -> bool {
        matches!(self, Self::Searxng)
    }
}

/// Everything a search needs, resolved from settings and the credential store.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub provider: SearchProvider,
    pub api_key: Option<String>,
    /// Instance URL, for providers that are addressed rather than hosted.
    pub base_url: Option<String>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            provider: SearchProvider::Duckduckgo,
            api_key: None,
            base_url: None,
        }
    }
}

impl SearchConfig {
    /// Whether this configuration can actually run a search.
    ///
    /// Checked before a search is attempted so a missing key is a clear message
    /// rather than a 401 from someone else's API.
    pub fn validate(&self) -> Result<()> {
        if self.provider.needs_api_key() && self.api_key.as_deref().unwrap_or("").trim().is_empty() {
            return Err(KuroError::bad_request(format!(
                "{} needs an API key. Add one in Settings, or switch to DuckDuckGo, which needs none.",
                self.provider.as_str()
            )));
        }
        if self.provider.needs_base_url() && self.base_url.as_deref().unwrap_or("").trim().is_empty()
        {
            return Err(KuroError::bad_request(
                "SearXNG needs the URL of an instance. Add one in Settings.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Run a search and return the results, best first.
pub async fn search(
    client: &reqwest::Client,
    config: &SearchConfig,
    query: &str,
    max_results: usize,
) -> Result<Vec<SearchResult>> {
    let query = query.trim();
    if query.is_empty() {
        return Err(KuroError::bad_request("the search query is empty"));
    }
    config.validate()?;

    let wanted = max_results.clamp(1, RESULT_LIMIT);

    let results = match config.provider {
        SearchProvider::Duckduckgo => duckduckgo(client, query).await?,
        SearchProvider::Brave => brave(client, config, query, wanted).await?,
        SearchProvider::Tavily => tavily(client, config, query, wanted).await?,
        SearchProvider::Searxng => searxng(client, config, query).await?,
    };

    Ok(results.into_iter().take(wanted).collect())
}

/* ---------- DuckDuckGo ---------- */

/// The no-JavaScript endpoint. Its markup is plain enough to read without a
/// parser, and it does not require a key.
const DDG_ENDPOINT: &str = "https://html.duckduckgo.com/html/";

async fn duckduckgo(client: &reqwest::Client, query: &str) -> Result<Vec<SearchResult>> {
    let response = client
        .post(DDG_ENDPOINT)
        // A browser-shaped Accept header; the API-oriented default gets a
        // different, harder-to-read page.
        .header("Accept", "text/html")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("q={}", html::percent_encode(query)))
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        return Err(KuroError::other(format!(
            "DuckDuckGo returned {status}. It rate-limits heavy use; \
             configuring Brave, Tavily or a SearXNG instance in Settings avoids that."
        )));
    }

    let results = parse_duckduckgo(&body);
    if results.is_empty() && !mentions_no_results(&body) {
        return Err(KuroError::other(
            "could not read DuckDuckGo's response. Its page layout may have changed — \
             configure Brave, Tavily or SearXNG in Settings for a stable API.",
        ));
    }

    Ok(results)
}

/// Pull results out of the HTML page.
///
/// Keyed on the *shape* of the document rather than on class names, which
/// DuckDuckGo renames, or on its `/l/?uddg=` redirect, which it has now dropped:
/// a result is an outbound link to somewhere that is not DuckDuckGo, and each one
/// appears several times in a row — as an icon, a title, a displayed URL and a
/// snippet. Grouping the repeats by URL recovers all three fields without caring
/// which class each carries.
fn parse_duckduckgo(body: &str) -> Vec<SearchResult> {
    // Insertion-ordered: the first time a URL appears is its rank.
    let mut order: Vec<String> = Vec::new();
    let mut texts: Vec<(String, Vec<String>)> = Vec::new();

    for (href, text) in html::anchors(body) {
        let Some(target) = result_target(&href) else {
            continue;
        };

        match texts.iter_mut().find(|(url, _)| *url == target) {
            Some((_, held)) => {
                if !text.is_empty() {
                    held.push(text);
                }
            }
            None => {
                order.push(target.clone());
                texts.push((target, if text.is_empty() { Vec::new() } else { vec![text] }));
            }
        }
    }

    order
        .into_iter()
        .take(RESULT_LIMIT)
        .map(|url| {
            let held = texts
                .iter()
                .find(|(held_url, _)| *held_url == url)
                .map(|(_, texts)| texts.as_slice())
                .unwrap_or(&[]);

            let (title, snippet) = split_title_and_snippet(held, &url);
            SearchResult {
                title: truncate(&title, 200),
                url,
                snippet: truncate(&snippet, MAX_SNIPPET_CHARS),
            }
        })
        .collect()
}

/// Decide which of a result's anchor texts is the title and which the snippet.
///
/// The longest is the snippet — a description is always longer than a headline.
/// The title is the longest of what remains that is not just the URL written out,
/// which is what the third link in each result block contains.
fn split_title_and_snippet(texts: &[String], url: &str) -> (String, String) {
    let host = host_of(url);

    let mut ranked: Vec<&String> = texts.iter().collect();
    ranked.sort_by_key(|text| std::cmp::Reverse(text.chars().count()));

    let snippet = ranked.first().map(|text| text.to_string()).unwrap_or_default();

    let title = ranked
        .iter()
        .skip(1)
        .find(|text| !is_displayed_url(text, &host))
        .map(|text| text.to_string())
        // Some layouts give the title and nothing else; when only one text exists
        // it is the title, not the snippet.
        .unwrap_or_else(|| if texts.len() == 1 { snippet.clone() } else { host.clone() });

    // A single text is a title, so it must not also be reported as a description.
    if texts.len() <= 1 {
        return (title, String::new());
    }
    (title, snippet)
}

/// True for the "tokio.rs/tokio/tutorial" line DuckDuckGo prints under a result.
fn is_displayed_url(text: &str, host: &str) -> bool {
    let normalised = text.replace(' ', "").to_ascii_lowercase();
    normalised.starts_with(&host.to_ascii_lowercase())
        || normalised.starts_with(&format!("www.{}", host.to_ascii_lowercase()))
}

/// The outbound URL a result link points at.
///
/// Handles the current direct form and the older `/l/?uddg=` redirect, because a
/// regional or cached deployment may still serve either.
fn result_target(href: &str) -> Option<String> {
    if href.contains("uddg=") {
        let after = href.split("uddg=").nth(1)?;
        let encoded = after.split('&').next()?;
        let decoded = html::percent_decode(encoded);
        return is_outbound(&decoded).then_some(decoded);
    }

    is_outbound(href).then(|| href.to_string())
}

/// Whether a URL leaves DuckDuckGo. Its own pages, assets and ad redirects do not
/// count as results.
fn is_outbound(url: &str) -> bool {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return false;
    }
    let host = host_of(url).to_ascii_lowercase();
    !(host == "duckduckgo.com" || host.ends_with(".duckduckgo.com"))
}

/// True when the page is genuinely reporting an empty result set, as opposed to
/// having a layout this code failed to read.
fn mentions_no_results(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("no results") || lower.contains("no-results")
}

/* ---------- Brave ---------- */

const BRAVE_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";

#[derive(Debug, Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWeb>,
}

#[derive(Debug, Deserialize)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(Debug, Deserialize)]
struct BraveResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

async fn brave(
    client: &reqwest::Client,
    config: &SearchConfig,
    query: &str,
    wanted: usize,
) -> Result<Vec<SearchResult>> {
    let key = config.api_key.as_deref().unwrap_or_default();
    let response = client
        .get(BRAVE_ENDPOINT)
        .query(&[("q", query), ("count", &wanted.to_string())])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", key)
        .send()
        .await?;

    let parsed: BraveResponse = provider_json(response, "Brave").await?;

    Ok(parsed
        .web
        .map(|web| web.results)
        .unwrap_or_default()
        .into_iter()
        .filter(|result| !result.url.is_empty())
        .map(|result| SearchResult {
            title: truncate(&result.title, 200),
            url: result.url,
            snippet: truncate(&html::to_text(&result.description), MAX_SNIPPET_CHARS),
        })
        .collect())
}

/* ---------- Tavily ---------- */

const TAVILY_ENDPOINT: &str = "https://api.tavily.com/search";

#[derive(Debug, Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    results: Vec<TavilyResult>,
}

#[derive(Debug, Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

async fn tavily(
    client: &reqwest::Client,
    config: &SearchConfig,
    query: &str,
    wanted: usize,
) -> Result<Vec<SearchResult>> {
    let key = config.api_key.as_deref().unwrap_or_default();
    let response = client
        .post(TAVILY_ENDPOINT)
        .bearer_auth(key)
        .json(&serde_json::json!({
            "query": query,
            "max_results": wanted,
            "search_depth": "basic",
        }))
        .send()
        .await?;

    let parsed: TavilyResponse = provider_json(response, "Tavily").await?;

    Ok(parsed
        .results
        .into_iter()
        .filter(|result| !result.url.is_empty())
        .map(|result| SearchResult {
            title: truncate(&result.title, 200),
            url: result.url,
            snippet: truncate(&result.content, MAX_SNIPPET_CHARS),
        })
        .collect())
}

/* ---------- SearXNG ---------- */

#[derive(Debug, Deserialize)]
struct SearxResponse {
    #[serde(default)]
    results: Vec<SearxResult>,
}

#[derive(Debug, Deserialize)]
struct SearxResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

async fn searxng(
    client: &reqwest::Client,
    config: &SearchConfig,
    query: &str,
) -> Result<Vec<SearchResult>> {
    let base = config
        .base_url
        .as_deref()
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/');

    let mut request = client
        .get(format!("{base}/search"))
        .query(&[("q", query), ("format", "json")]);

    // Some instances sit behind a token even though the software itself takes
    // none, so send one when it is configured.
    if let Some(key) = config.api_key.as_deref().filter(|key| !key.trim().is_empty()) {
        request = request.bearer_auth(key);
    }

    let response = request.send().await?;
    let parsed: SearxResponse = provider_json(response, "SearXNG").await?;

    Ok(parsed
        .results
        .into_iter()
        .filter(|result| !result.url.is_empty())
        .map(|result| SearchResult {
            title: truncate(&result.title, 200),
            url: result.url,
            snippet: truncate(&result.content, MAX_SNIPPET_CHARS),
        })
        .collect())
}

/* ---------- Shared ---------- */

/// Decode a provider's JSON, turning an HTTP failure into a message that names
/// the provider and quotes what it said.
async fn provider_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    provider: &str,
) -> Result<T> {
    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        let detail = truncate(body.trim(), 300);
        return Err(match status.as_u16() {
            401 | 403 => KuroError::bad_request(format!(
                "{provider} rejected the API key ({status}). Check it in Settings."
            )),
            429 => KuroError::other(format!("{provider} rate-limited the request ({status}).")),
            _ => KuroError::other(format!("{provider} returned {status}: {detail}")),
        });
    }

    serde_json::from_str(&body).map_err(|error| {
        KuroError::other(format!("could not read {provider}'s response: {error}"))
    })
}

fn truncate(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let kept: String = trimmed.chars().take(limit).collect();
    format!("{}…", kept.trim_end())
}

fn host_of(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

/// Render results as the text a model reads.
///
/// Numbered, with the URL on its own line, because that is the shape models are
/// most reliable at citing back.
pub fn format_for_model(query: &str, results: &[SearchResult]) -> String {
    if results.is_empty() {
        return format!("No web results for \"{query}\".");
    }

    let mut out = format!("Web results for \"{query}\":\n");
    for (index, result) in results.iter().enumerate() {
        out.push_str(&format!("\n{}. {}\n   {}\n", index + 1, result.title, result.url));
        if !result.snippet.is_empty() {
            out.push_str(&format!("   {}\n", result.snippet));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_provider_needs_no_setup() {
        let config = SearchConfig::default();
        assert_eq!(config.provider, SearchProvider::Duckduckgo);
        assert!(config.validate().is_ok(), "search must work out of the box");
    }

    #[test]
    fn provider_names_round_trip_and_reject_nonsense() {
        for provider in [
            SearchProvider::Duckduckgo,
            SearchProvider::Brave,
            SearchProvider::Tavily,
            SearchProvider::Searxng,
        ] {
            assert_eq!(SearchProvider::parse(provider.as_str()), Some(provider));
        }
        assert_eq!(SearchProvider::parse("  BRAVE "), Some(SearchProvider::Brave));
        assert_eq!(SearchProvider::parse("altavista"), None);
    }

    #[test]
    fn a_keyed_provider_without_a_key_says_so_before_calling_out() {
        let config = SearchConfig {
            provider: SearchProvider::Brave,
            api_key: None,
            base_url: None,
        };
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("API key"), "got: {error}");
        assert!(error.contains("DuckDuckGo"), "the message should offer a way forward");
    }

    #[test]
    fn a_whitespace_key_counts_as_missing() {
        let config = SearchConfig {
            provider: SearchProvider::Tavily,
            api_key: Some("   ".to_string()),
            base_url: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn searxng_requires_an_instance_url() {
        let config = SearchConfig {
            provider: SearchProvider::Searxng,
            api_key: None,
            base_url: None,
        };
        assert!(config.validate().unwrap_err().to_string().contains("instance"));

        let configured = SearchConfig {
            base_url: Some("http://localhost:8888".to_string()),
            ..config
        };
        assert!(configured.validate().is_ok());
    }

    #[test]
    fn accepts_a_direct_result_link() {
        assert_eq!(
            result_target("https://tokio.rs/tokio/tutorial").as_deref(),
            Some("https://tokio.rs/tokio/tutorial")
        );
    }

    #[test]
    fn still_unwraps_the_older_redirect_form() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=abc";
        assert_eq!(result_target(href).as_deref(), Some("https://example.com/page"));
    }

    #[test]
    fn ignores_links_that_are_not_results() {
        assert_eq!(result_target("/settings"), None);
        assert_eq!(result_target("//duckduckgo.com/favicon.ico"), None);
        assert_eq!(result_target("https://duckduckgo.com/about"), None);
        assert_eq!(result_target("https://html.duckduckgo.com/html/"), None);
        // An ad redirect through DuckDuckGo's own host is not a result either.
        assert_eq!(result_target("https://duckduckgo.com/y.js?ad=1"), None);
        // A redirect to something that is not http is not a usable result.
        assert_eq!(result_target("//duckduckgo.com/l/?uddg=javascript%3Aalert(1)"), None);
    }

    /// The markup DuckDuckGo actually serves: four links per result, direct hrefs.
    #[test]
    fn parses_the_current_results_page() {
        let page = r#"
            <div class="result results_links">
              <div class="result__icon">
                <a href="https://tokio.rs/tokio/tutorial/async"><img src="i.png"/></a>
              </div>
              <h2 class="result__title">
                <a class="result__a" href="https://tokio.rs/tokio/tutorial/async">Async in depth | Tokio</a>
              </h2>
              <a class="result__snippet" href="https://tokio.rs/tokio/tutorial/async">Tokio is an asynchronous runtime for the Rust programming language.</a>
              <div class="result__extras">
                <a class="result__url" href="https://tokio.rs/tokio/tutorial/async">tokio.rs/tokio/tutorial/async</a>
              </div>
            </div>
            <div class="result results_links">
              <h2><a class="result__a" href="https://rust-lang.github.io/async-book/">Async Book</a></h2>
              <a class="result__snippet" href="https://rust-lang.github.io/async-book/">The ecosystem chapter, covering runtimes.</a>
              <a class="result__url" href="https://rust-lang.github.io/async-book/">rust-lang.github.io/async-book</a>
            </div>
        "#;

        let results = parse_duckduckgo(page);

        assert_eq!(results.len(), 2, "one entry per distinct target");
        assert_eq!(results[0].url, "https://tokio.rs/tokio/tutorial/async");
        assert_eq!(results[0].title, "Async in depth | Tokio");
        assert_eq!(
            results[0].snippet, "Tokio is an asynchronous runtime for the Rust programming language.",
            "the description must not be mistaken for the displayed URL"
        );
        assert_eq!(results[1].title, "Async Book");
        assert!(results[1].snippet.contains("ecosystem"));
    }

    #[test]
    fn a_result_with_only_a_title_reports_no_snippet() {
        let page = r#"<a class="result__a" href="https://a.example">Only a title</a>"#;
        let results = parse_duckduckgo(page);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Only a title");
        assert_eq!(results[0].snippet, "", "a title must not be repeated as its own description");
    }

    #[test]
    fn the_displayed_url_is_never_used_as_a_title() {
        let page = r#"
            <a class="result__a" href="https://tokio.rs/x">Tokio</a>
            <a class="result__url" href="https://tokio.rs/x">tokio.rs/x</a>
            <a class="result__snippet" href="https://tokio.rs/x">A much longer description of the page.</a>
        "#;

        let results = parse_duckduckgo(page);

        assert_eq!(results[0].title, "Tokio");
        assert!(results[0].snippet.contains("longer description"));
    }

    #[test]
    fn recognises_a_displayed_url_including_a_www_prefix() {
        assert!(is_displayed_url("tokio.rs/tutorial", "tokio.rs"));
        assert!(is_displayed_url("www.example.com/a", "example.com"));
        assert!(is_displayed_url("example.com › docs › intro", "example.com"));
        assert!(!is_displayed_url("Async in depth", "tokio.rs"));
    }

    #[test]
    fn ranking_follows_first_appearance_not_text_length() {
        let page = r#"
            <a href="https://first.example">First</a>
            <a href="https://second.example">Second with a much longer title</a>
        "#;

        let results = parse_duckduckgo(page);

        assert_eq!(results[0].url, "https://first.example");
        assert_eq!(results[1].url, "https://second.example");
    }

    #[test]
    fn a_page_with_no_recognisable_results_parses_as_empty() {
        assert!(parse_duckduckgo("<html><body>nothing here</body></html>").is_empty());
    }

    #[test]
    fn distinguishes_an_empty_result_set_from_an_unreadable_page() {
        assert!(mentions_no_results("<div>No results found for that query.</div>"));
        assert!(!mentions_no_results("<div>Some completely new layout</div>"));
    }

    #[test]
    fn snippets_are_truncated_with_an_ellipsis() {
        let long = "word ".repeat(300);
        let shortened = truncate(&long, MAX_SNIPPET_CHARS);
        assert!(shortened.chars().count() <= MAX_SNIPPET_CHARS + 1);
        assert!(shortened.ends_with('…'));
    }

    #[test]
    fn truncation_leaves_short_text_untouched() {
        assert_eq!(truncate("  brief  ", 100), "brief");
    }

    #[test]
    fn formats_results_so_a_model_can_cite_them() {
        let results = vec![SearchResult {
            title: "Rust".to_string(),
            url: "https://rust-lang.org".to_string(),
            snippet: "A language".to_string(),
        }];

        let rendered = format_for_model("rust", &results);

        assert!(rendered.contains("1. Rust"));
        assert!(rendered.contains("https://rust-lang.org"));
        assert!(rendered.contains("A language"));
    }

    #[test]
    fn formats_an_empty_result_set_as_a_statement_not_a_blank() {
        let rendered = format_for_model("obscure query", &[]);
        assert!(rendered.contains("No web results"));
        assert!(rendered.contains("obscure query"));
    }

    #[test]
    fn falls_back_to_the_host_when_a_result_has_no_title() {
        assert_eq!(host_of("https://example.com/a/b?c=d"), "example.com");
    }

    #[tokio::test]
    async fn an_empty_query_is_rejected_without_a_network_call() {
        let client = reqwest::Client::new();
        let error = search(&client, &SearchConfig::default(), "   ", 5)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("empty"));
    }
}
