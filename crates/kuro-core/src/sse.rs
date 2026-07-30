//! Server-sent event parsing.
//!
//! Used in both directions: the server parses the engine's OpenAI-style stream,
//! and the CLI parses Kuro's own event stream. Chunks arrive on arbitrary byte
//! boundaries, so bytes are buffered and only complete events are decoded —
//! decoding early would split multi-byte characters.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` name, when the sender set one.
    pub event: Option<String>,
    pub data: String,
}

/// Remove and return every complete event in `buffer`.
///
/// Incomplete trailing bytes stay in the buffer for the next chunk.
pub fn drain_events(buffer: &mut Vec<u8>) -> Vec<SseEvent> {
    let mut events = Vec::new();

    while let Some((position, separator_length)) = find_block_end(buffer) {
        let block: Vec<u8> = buffer.drain(..position + separator_length).collect();
        let text = String::from_utf8_lossy(&block);

        let mut name = None;
        let mut data_lines: Vec<&str> = Vec::new();

        for line in text.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                name = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("data:") {
                data_lines.push(value.strip_prefix(' ').unwrap_or(value));
            }
        }

        if !data_lines.is_empty() {
            events.push(SseEvent {
                event: name,
                // Multi-line payloads are joined with newlines, per the spec.
                data: data_lines.join("\n"),
            });
        }
    }

    events
}

/// Position and length of the first event separator, handling `\n\n` and
/// `\r\n\r\n`.
fn find_block_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = find_subslice(buffer, b"\n\n").map(|position| (position, 2));
    let crlf = find_subslice(buffer, b"\r\n\r\n").map(|position| (position, 4));

    match (lf, crlf) {
        (Some(lf), Some(crlf)) => Some(if lf.0 <= crlf.0 { lf } else { crlf }),
        (Some(lf), None) => Some(lf),
        (None, Some(crlf)) => Some(crlf),
        (None, None) => None,
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Whether a payload is the OpenAI stream terminator rather than content.
pub fn is_done(data: &str) -> bool {
    data.trim() == "[DONE]"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_of(events: &[SseEvent]) -> Vec<&str> {
        events.iter().map(|event| event.data.as_str()).collect()
    }

    #[test]
    fn extracts_complete_events_only() {
        let mut buffer = b"data: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: {\"partial\"".to_vec();

        let events = drain_events(&mut buffer);

        assert_eq!(data_of(&events), vec!["{\"a\":1}", "{\"b\":2}"]);
        assert_eq!(
            String::from_utf8_lossy(&buffer),
            "data: {\"partial\"",
            "an incomplete event must stay buffered"
        );
    }

    #[test]
    fn captures_event_names() {
        let mut buffer = b"event: token\ndata: {\"content\":\"hi\"}\n\n".to_vec();

        let events = drain_events(&mut buffer);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("token"));
        assert_eq!(events[0].data, "{\"content\":\"hi\"}");
    }

    #[test]
    fn handles_events_split_across_chunk_boundaries() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"data: {\"content\":\"hel");
        assert!(drain_events(&mut buffer).is_empty());

        buffer.extend_from_slice(b"lo\"}\n\n");
        assert_eq!(data_of(&drain_events(&mut buffer)), vec!["{\"content\":\"hello\"}"]);
    }

    #[test]
    fn does_not_corrupt_multibyte_characters_split_across_chunks() {
        // The three bytes of "日" arrive in two separate chunks.
        let japanese = "日".as_bytes();
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"data: {\"c\":\"");
        buffer.extend_from_slice(&japanese[..1]);
        assert!(drain_events(&mut buffer).is_empty());

        buffer.extend_from_slice(&japanese[1..]);
        buffer.extend_from_slice(b"\"}\n\n");

        assert_eq!(data_of(&drain_events(&mut buffer)), vec!["{\"c\":\"日\"}"]);
    }

    #[test]
    fn accepts_carriage_return_separators() {
        let mut buffer = b"data: {\"a\":1}\r\n\r\n".to_vec();
        assert_eq!(data_of(&drain_events(&mut buffer)), vec!["{\"a\":1}"]);
    }

    #[test]
    fn ignores_comment_only_blocks_such_as_keep_alives() {
        let mut buffer = b": keep-alive\n\ndata: {\"a\":1}\n\n".to_vec();
        assert_eq!(data_of(&drain_events(&mut buffer)), vec!["{\"a\":1}"]);
    }

    #[test]
    fn joins_multi_line_payloads() {
        let mut buffer = b"data: first\ndata: second\n\n".to_vec();
        assert_eq!(data_of(&drain_events(&mut buffer)), vec!["first\nsecond"]);
    }

    #[test]
    fn recognises_the_terminator() {
        assert!(is_done("[DONE]"));
        assert!(is_done(" [DONE] "));
        assert!(!is_done("{\"a\":1}"));
    }
}
