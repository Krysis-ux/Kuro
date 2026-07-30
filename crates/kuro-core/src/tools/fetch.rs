//! Reading a web page.
//!
//! Search gives a model titles and snippets; this is how it reads the actual
//! page. Kept separate from search because it is useful on its own — a user can
//! paste a URL and ask about it without a search provider being configured at
//! all.
//!
//! The safety concern here is that the model chooses the URL. A model that has
//! been fed a malicious page can be talked into fetching a loopback or private
//! address, which on this machine means Kuro's own API and every engine port.
//! [`is_permitted`] is what stops that.

use std::net::IpAddr;
use std::time::Duration;

use crate::tools::html;
use crate::{KuroError, Result};

/// A page taking longer than this is not worth the wait mid-conversation.
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
/// Stop reading a response after this much text. Enough for an article, small
/// enough that one fetch cannot fill a context window on its own.
const MAX_CHARS: usize = 20_000;

/// A page, reduced to text.
#[derive(Debug, Clone)]
pub struct FetchedPage {
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub truncated: bool,
}

/// Fetch a URL and return its readable text.
pub async fn fetch_url(client: &reqwest::Client, url: &str) -> Result<FetchedPage> {
    let url = url.trim();
    is_permitted(url)?;

    let response = client
        .get(url)
        .header("Accept", "text/html,text/plain,application/json;q=0.9")
        .timeout(FETCH_TIMEOUT)
        .send()
        .await?;

    let status = response.status();
    let final_url = response.url().to_string();

    // A redirect can land somewhere the original URL would not have been allowed
    // to reach, so the destination is checked too.
    is_permitted(&final_url)?;

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    if !status.is_success() {
        return Err(KuroError::other(format!("{url} returned {status}")));
    }

    if is_binary(&content_type) {
        return Err(KuroError::bad_request(format!(
            "{url} is {content_type}, which has no text to read"
        )));
    }

    let body = response.text().await?;
    let is_markup = content_type.contains("html") || body.trim_start().starts_with('<');

    let text = if is_markup {
        html::to_text(&body)
    } else {
        body.trim().to_string()
    };

    let title = is_markup.then(|| extract_title(&body)).flatten();
    let truncated = text.chars().count() > MAX_CHARS;

    Ok(FetchedPage {
        url: final_url,
        title,
        text: text.chars().take(MAX_CHARS).collect(),
        truncated,
    })
}

/// Reject anything that is not a public web address.
///
/// This is the guard on a model-chosen URL. Loopback and the private ranges are
/// refused because on this machine they reach Kuro's own API, the engine ports,
/// and whatever else the user happens to be running.
pub fn is_permitted(url: &str) -> Result<()> {
    let lower = url.to_ascii_lowercase();

    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(KuroError::bad_request(
            "only http and https URLs can be fetched",
        ));
    }

    let after_scheme = lower.split("://").nth(1).unwrap_or_default();
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    // Drop any userinfo, which is a classic way to disguise the real host.
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = strip_port(host_port);

    if host.is_empty() {
        return Err(KuroError::bad_request("that URL has no host"));
    }

    if is_local_name(host) {
        return Err(KuroError::bad_request(format!(
            "`{host}` is on this machine or local network, which tools are not allowed to reach"
        )));
    }

    if let Ok(address) = host.parse::<IpAddr>() {
        if !is_public_address(&address) {
            return Err(KuroError::bad_request(format!(
                "`{host}` is a private address, which tools are not allowed to reach"
            )));
        }
    }

    Ok(())
}

/// Split a trailing `:port`, leaving an IPv6 literal's colons alone.
fn strip_port(host_port: &str) -> &str {
    if let Some(end) = host_port.strip_prefix('[').and_then(|rest| rest.find(']')) {
        return &host_port[1..=end];
    }
    match host_port.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => host_port,
    }
}

fn is_local_name(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        // The cloud metadata endpoint, which is an address rather than a name but
        // is worth naming explicitly.
        || host == "metadata.google.internal"
}

fn is_public_address(address: &IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            !v4.is_loopback()
                && !v4.is_private()
                && !v4.is_link_local()
                && !v4.is_broadcast()
                && !v4.is_documentation()
                && !v4.is_unspecified()
                // 100.64.0.0/10, used by carrier NAT and by Tailscale.
                && !(v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
                // 169.254.169.254 and the rest of link-local is already covered,
                // but 0.0.0.0/8 is not.
                && v4.octets()[0] != 0
        }
        IpAddr::V6(v6) => {
            !v6.is_loopback()
                && !v6.is_unspecified()
                // fc00::/7 unique-local and fe80::/10 link-local.
                && !(v6.segments()[0] & 0xfe00 == 0xfc00)
                && !(v6.segments()[0] & 0xffc0 == 0xfe80)
                // An IPv4-mapped address would otherwise bypass the checks above.
                && v6.to_ipv4_mapped().map(|v4| is_public_address(&IpAddr::V4(v4))).unwrap_or(true)
        }
    }
}

fn is_binary(content_type: &str) -> bool {
    const TEXTUAL: &[&str] = &["text/", "json", "xml", "javascript", "csv", "yaml", "markdown"];
    if content_type.is_empty() {
        return false;
    }
    !TEXTUAL.iter().any(|kind| content_type.contains(kind))
}

fn extract_title(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;

    let title = html::to_text(&body[start..end]);
    let trimmed = title.trim();
    (!trimmed.is_empty()).then(|| trimmed.chars().take(200).collect())
}

/// Render a page as the text a model reads.
pub fn format_for_model(page: &FetchedPage) -> String {
    let mut out = String::new();
    if let Some(title) = &page.title {
        out.push_str(&format!("{title}\n"));
    }
    out.push_str(&format!("{}\n\n", page.url));
    out.push_str(&page.text);
    if page.truncated {
        out.push_str("\n\n[truncated]");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_ordinary_public_urls() {
        for url in [
            "https://example.com",
            "http://example.com/path?q=1",
            "https://sub.domain.example.co.uk/a",
            "https://93.184.216.34/",
        ] {
            assert!(is_permitted(url).is_ok(), "should allow {url}");
        }
    }

    #[test]
    fn refuses_non_http_schemes() {
        for url in ["file:///etc/passwd", "ftp://example.com", "javascript:alert(1)", "data:text/html,x"] {
            assert!(is_permitted(url).is_err(), "should refuse {url}");
        }
    }

    #[test]
    fn refuses_loopback_so_a_tool_cannot_reach_kuros_own_api() {
        for url in [
            "http://localhost:8420/api/settings",
            "http://127.0.0.1:8420/api/settings",
            "http://127.0.0.1:39200/v1/models",
            "http://[::1]:8420/",
            "http://0.0.0.0:8420/",
        ] {
            assert!(is_permitted(url).is_err(), "should refuse {url}");
        }
    }

    #[test]
    fn refuses_private_and_link_local_ranges() {
        for url in [
            "http://192.168.1.1/",
            "http://10.0.0.5/admin",
            "http://172.16.4.4/",
            "http://169.254.169.254/latest/meta-data/",
            "http://100.100.0.1/",
            "http://[fd00::1]/",
            "http://[fe80::1]/",
        ] {
            assert!(is_permitted(url).is_err(), "should refuse {url}");
        }
    }

    #[test]
    fn refuses_local_hostnames() {
        for url in ["http://router.local/", "http://db.internal/", "http://metadata.google.internal/"] {
            assert!(is_permitted(url).is_err(), "should refuse {url}");
        }
    }

    #[test]
    fn userinfo_cannot_disguise_a_local_host() {
        assert!(
            is_permitted("http://example.com@127.0.0.1/").is_err(),
            "the host is what follows the @, not what precedes it"
        );
    }

    #[test]
    fn an_ipv4_mapped_ipv6_loopback_is_still_loopback() {
        assert!(is_permitted("http://[::ffff:127.0.0.1]/").is_err());
    }

    #[test]
    fn a_url_without_a_host_is_rejected() {
        assert!(is_permitted("http:///path").is_err());
    }

    #[test]
    fn separates_a_port_from_a_host_including_ipv6() {
        assert_eq!(strip_port("example.com:8080"), "example.com");
        assert_eq!(strip_port("example.com"), "example.com");
        assert_eq!(strip_port("[::1]:8420"), "::1");
        assert_eq!(strip_port("[::1]"), "::1");
    }

    #[test]
    fn recognises_content_that_has_no_text_to_read() {
        assert!(is_binary("image/png"));
        assert!(is_binary("application/octet-stream"));
        assert!(!is_binary("text/html; charset=utf-8"));
        assert!(!is_binary("application/json"));
        assert!(!is_binary(""), "a missing content type should be attempted, not refused");
    }

    #[test]
    fn reads_the_document_title() {
        let body = "<html><head><title>  A Page  </title></head><body>x</body></html>";
        assert_eq!(extract_title(body).as_deref(), Some("A Page"));
        assert_eq!(extract_title("<html><body>no title</body></html>"), None);
    }

    #[test]
    fn a_title_with_markup_inside_is_still_read() {
        let body = "<title>Home &mdash; <b>Example</b></title>";
        assert_eq!(extract_title(body).as_deref(), Some("Home — Example"));
    }

    #[test]
    fn formatting_marks_a_truncated_page() {
        let page = FetchedPage {
            url: "https://example.com".to_string(),
            title: Some("Example".to_string()),
            text: "body text".to_string(),
            truncated: true,
        };

        let rendered = format_for_model(&page);

        assert!(rendered.starts_with("Example"));
        assert!(rendered.contains("https://example.com"));
        assert!(rendered.ends_with("[truncated]"));
    }
}
