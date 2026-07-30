//! Turning HTML into something a model can read.
//!
//! Not a parser. It strips markup, drops the contents of elements that only
//! exist for the browser, and decodes the handful of entities that actually turn
//! up in prose. A real DOM parser would be more correct, but the job here is to
//! recover readable text and the failure mode of this approach — an occasional
//! stray character — costs a token, not a wrong answer.

/// Elements whose contents are never prose.
const SKIPPED_ELEMENTS: &[&str] = &["script", "style", "noscript", "svg", "head", "template"];

/// Extract readable text from an HTML document.
pub fn to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 4);
    let bytes = html.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != b'<' {
            let start = index;
            while index < bytes.len() && bytes[index] != b'<' {
                index += 1;
            }
            push_text(&mut out, &html[start..index]);
            continue;
        }

        let Some(tag_end) = html[index..].find('>').map(|offset| index + offset) else {
            // Unterminated tag: everything left is markup, so stop.
            break;
        };
        let tag = &html[index + 1..tag_end];
        let name = tag_name(tag);

        if let Some(element) = SKIPPED_ELEMENTS.iter().find(|element| **element == name) {
            index = skip_element(html, tag_end + 1, element);
            continue;
        }

        // Block-level markup is the only structure worth keeping, as a break.
        if is_block(&name) {
            push_break(&mut out);
        }

        index = tag_end + 1;
    }

    collapse_blank_lines(out.trim())
}

/// Every `href` in the document, in order, with entities decoded.
pub fn links(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut cursor = 0;

    while let Some(offset) = lower[cursor..].find("href=") {
        let start = cursor + offset + "href=".len();
        cursor = start;

        let Some(quote) = html[start..].chars().next() else { break };
        if quote != '"' && quote != '\'' {
            continue;
        }

        let value_start = start + quote.len_utf8();
        let Some(end) = html[value_start..].find(quote).map(|o| value_start + o) else {
            break;
        };

        found.push(decode_entities(&html[value_start..end]));
        cursor = end;
    }

    found
}

/// Every anchor in the document as `(href, inner text)`, in order.
///
/// One pass rather than searching for a known `href`, because an href in the
/// source may be entity-encoded while the value a caller holds has already been
/// decoded — matching the two up by string is a bug waiting to happen. Walking the
/// document instead means the pairing is always correct.
pub fn anchors(html: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut cursor = 0;

    while let Some(offset) = lower[cursor..].find("<a") {
        let tag_start = cursor + offset;
        // `<article>` also starts with `<a`; require a delimiter after it.
        let next = html[tag_start + 2..].chars().next();
        if !matches!(next, Some(c) if c.is_whitespace() || c == '>') {
            cursor = tag_start + 2;
            continue;
        }

        let Some(tag_end) = html[tag_start..].find('>').map(|o| tag_start + o) else {
            break;
        };
        let href = attribute(&html[tag_start..tag_end], "href");

        let body_start = tag_end + 1;
        let close = lower[body_start..]
            .find("</a")
            .map(|o| body_start + o)
            .unwrap_or(html.len());

        if let Some(href) = href {
            found.push((href, to_text(&html[body_start..close]).trim().to_string()));
        }

        cursor = close.max(tag_start + 2);
    }

    found
}

/// Read one attribute out of a tag's source, with entities decoded.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut cursor = 0;

    // Looped rather than taking the first hit, because `data-href` contains
    // `href` and would otherwise match.
    while let Some(offset) = lower[cursor..].find(name) {
        let at = cursor + offset;
        cursor = at + name.len();

        let preceded_by_delimiter = at == 0
            || tag[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace());
        if !preceded_by_delimiter {
            continue;
        }

        let after = tag[cursor..].trim_start();
        let Some(rest) = after.strip_prefix('=') else { continue };
        let rest = rest.trim_start();

        let Some(quote) = rest.chars().next() else { continue };
        if quote != '"' && quote != '\'' {
            // Unquoted attribute value: read to the next whitespace.
            let value: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
            return (!value.is_empty()).then(|| decode_entities(&value));
        }

        let value_start = quote.len_utf8();
        let end = rest[value_start..].find(quote)? + value_start;
        return Some(decode_entities(&rest[value_start..end]));
    }

    None
}

/// Advance past a whole element, including its contents.
fn skip_element(html: &str, from: usize, element: &str) -> usize {
    let closing = format!("</{element}");
    let lower = html.to_ascii_lowercase();

    match lower[from..].find(&closing) {
        Some(offset) => {
            let close_start = from + offset;
            match html[close_start..].find('>') {
                Some(gt) => close_start + gt + 1,
                None => html.len(),
            }
        }
        // No closing tag: treat the rest of the document as part of the element,
        // which is what a browser does too.
        None => html.len(),
    }
}

fn tag_name(tag: &str) -> String {
    tag.trim_start_matches('/')
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "br"
            | "li"
            | "tr"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "table"
            | "blockquote"
            | "pre"
            | "ul"
            | "ol"
            | "nav"
            | "aside"
            | "main"
    )
}

fn push_text(out: &mut String, raw: &str) {
    let decoded = decode_entities(raw);
    for part in decoded.split_whitespace() {
        if !out.is_empty() && !out.ends_with(char::is_whitespace) {
            out.push(' ');
        }
        out.push_str(part);
    }
    // Preserve the fact that the source had trailing whitespace, so words either
    // side of an inline tag do not run together.
    if decoded.ends_with(char::is_whitespace) && !out.is_empty() && !out.ends_with(char::is_whitespace) {
        out.push(' ');
    }
}

fn push_break(out: &mut String) {
    if out.is_empty() || out.ends_with('\n') {
        return;
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out.push('\n');
}

fn decode_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }

    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;

    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];

        // Entities are short; anything longer is a literal ampersand.
        let Some(end) = after[..after.len().min(12)].find(';') else {
            out.push('&');
            rest = &after[1..];
            continue;
        };

        let entity = &after[1..end];
        match named_entity(entity) {
            Some(replacement) => out.push_str(replacement),
            None => match numeric_entity(entity) {
                Some(character) => out.push(character),
                None => {
                    out.push('&');
                    rest = &after[1..];
                    continue;
                }
            },
        }

        rest = &after[end + 1..];
    }

    out.push_str(rest);
    out
}

fn named_entity(entity: &str) -> Option<&'static str> {
    Some(match entity {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" | "#39" => "'",
        "nbsp" => " ",
        "hellip" => "…",
        "mdash" => "—",
        "ndash" => "–",
        "rsquo" | "lsquo" => "'",
        "rdquo" | "ldquo" => "\"",
        _ => return None,
    })
}

fn numeric_entity(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix('#')?;
    let code = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse::<u32>().ok()?,
    };
    char::from_u32(code)
}

/// Collapse runs of blank lines, which markup-heavy pages produce in quantity.
fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;

    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(trimmed);
        out.push('\n');
    }

    out.trim_end().to_string()
}

/// Percent-decode a URL, which DuckDuckGo needs because it wraps every result
/// link in a redirect carrying the real target as a query parameter.
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = &input[index + 1..index + 3];
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).to_string()
}

/// Percent-encode a value for use in a query string.
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_keeps_words_apart() {
        assert_eq!(to_text("<p>Hello <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn discards_script_and_style_contents() {
        let html = "<style>body{color:red}</style><p>Visible</p><script>alert(1)</script>";
        assert_eq!(to_text(html), "Visible");
    }

    #[test]
    fn an_unclosed_script_does_not_leak_code() {
        let text = to_text("<p>before</p><script>var x = 1;");
        assert!(!text.contains("var x"), "got: {text}");
        assert!(text.contains("before"));
    }

    #[test]
    fn block_elements_become_line_breaks() {
        let text = to_text("<p>one</p><p>two</p>");
        assert_eq!(text, "one\ntwo");
    }

    #[test]
    fn collapses_runs_of_empty_markup() {
        let text = to_text("<div></div><div></div><p>only line</p><div></div>");
        assert_eq!(text, "only line");
    }

    #[test]
    fn decodes_the_entities_that_appear_in_prose() {
        assert_eq!(to_text("<p>Tom &amp; Jerry &mdash; 5 &lt; 6</p>"), "Tom & Jerry — 5 < 6");
        assert_eq!(to_text("<p>caf&#233; &#x2014; open</p>"), "café — open");
    }

    #[test]
    fn a_bare_ampersand_survives() {
        assert_eq!(to_text("<p>R&D and rock & roll</p>"), "R&D and rock & roll");
    }

    #[test]
    fn an_unterminated_tag_does_not_loop_forever() {
        assert_eq!(to_text("text <div"), "text");
    }

    #[test]
    fn collects_links_in_document_order() {
        let html = r#"<a href="https://a.example">A</a><a href='https://b.example'>B</a>"#;
        assert_eq!(links(html), vec!["https://a.example", "https://b.example"]);
    }

    #[test]
    fn link_extraction_decodes_entities_in_the_url() {
        let html = r#"<a href="https://e.example/?a=1&amp;b=2">x</a>"#;
        assert_eq!(links(html), vec!["https://e.example/?a=1&b=2"]);
    }

    #[test]
    fn pairs_every_link_with_its_anchor_text() {
        let html = r#"<a class="r" href="https://a.example"><b>Title</b> here</a>
                      <a href="https://b.example">Second</a>"#;

        let pairs = anchors(html);

        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("https://a.example".to_string(), "Title here".to_string()));
        assert_eq!(pairs[1].1, "Second");
    }

    #[test]
    fn an_empty_anchor_is_still_reported() {
        let pairs = anchors(r#"<a href="https://a.example"><img src="i.png"/></a>"#);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].1, "", "an icon link has no text but still has an href");
    }

    #[test]
    fn tags_that_merely_start_with_a_are_not_anchors() {
        let pairs = anchors(r#"<article><address>x</address></article><a href="https://a.example">A</a>"#);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "https://a.example");
    }

    #[test]
    fn anchor_hrefs_are_entity_decoded() {
        let pairs = anchors(r#"<a href="https://e.example/?a=1&amp;b=2">x</a>"#);
        assert_eq!(pairs[0].0, "https://e.example/?a=1&b=2");
    }

    #[test]
    fn an_unclosed_anchor_takes_the_rest_of_the_document() {
        let pairs = anchors(r#"<a href="https://a.example">dangling"#);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].1, "dangling");
    }

    #[test]
    fn reads_attributes_without_confusing_similar_names() {
        let tag = r#"a data-href="https://wrong.example" href="https://right.example" class="x""#;
        assert_eq!(attribute(tag, "href").as_deref(), Some("https://right.example"));
        assert_eq!(attribute(tag, "class").as_deref(), Some("x"));
        assert_eq!(attribute(tag, "rel"), None);
    }

    #[test]
    fn reads_single_quoted_and_unquoted_attributes() {
        assert_eq!(attribute("a href='https://a.example'", "href").as_deref(), Some("https://a.example"));
        assert_eq!(attribute("a href=https://a.example rel=x", "href").as_deref(), Some("https://a.example"));
    }

    #[test]
    fn percent_coding_round_trips() {
        let original = "rust async traits & things";
        assert_eq!(percent_decode(&percent_encode(original)), original);
    }

    #[test]
    fn percent_decoding_handles_utf8_and_malformed_input() {
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("100%"), "100%", "a trailing percent is literal");
        assert_eq!(percent_decode("a%ZZb"), "a%ZZb");
    }

    #[test]
    fn encoding_escapes_everything_a_query_string_cares_about() {
        assert_eq!(percent_encode("a b&c=d?e/f"), "a%20b%26c%3Dd%3Fe%2Ff");
    }
}
