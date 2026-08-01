//! Rendering saved memories for a model.
//!
//! Storage and retrieval live in `db::memories`; this is only the presentation
//! side, kept next to the other tools so the whole `remember`/`recall` pair reads
//! in one place.

use crate::db::MemoryRecord;

/// How many memories are worth injecting into a prompt unasked. Past this, the
/// model should call `recall` with something specific instead.
pub const MAX_PRELOADED: usize = 12;

/// Render recall results as the text the model reads.
pub fn format_for_model(query: &str, memories: &[MemoryRecord]) -> String {
    if memories.is_empty() {
        return if query.trim().is_empty() {
            "Nothing has been remembered yet.".to_string()
        } else {
            format!("Nothing remembered matches \"{query}\".")
        };
    }

    let mut out = String::from("Remembered:\n");
    for memory in memories {
        out.push_str(&format!("- {}", memory.content));
        if !memory.tags.is_empty() {
            out.push_str(&format!(" [{}]", memory.tags.join(", ")));
        }
        out.push('\n');
    }
    out
}

/// The block prepended to a conversation so the model starts out aware of what it
/// already knows.
///
/// Without this, memory only works when the model thinks to call `recall` — which
/// small models frequently do not. Preloading a short list makes the feature
/// behave the way a user expects it to.
pub fn preamble(memories: &[MemoryRecord]) -> Option<String> {
    preamble_with(None, memories)
}

/// The memory preamble, including anything the user wrote about themselves.
///
/// The two are kept visibly apart in the text. What somebody typed on purpose is
/// a standing instruction and should win; what a model saved during a
/// conversation is a recollection and might be stale or half-right. Merging them
/// into one list would give a note the model wrote itself the same weight as one
/// the user did.
pub fn preamble_with(about_you: Option<&str>, memories: &[MemoryRecord]) -> Option<String> {
    let about_you = about_you.map(str::trim).filter(|text| !text.is_empty());
    if about_you.is_none() && memories.is_empty() {
        return None;
    }

    let mut out = String::new();

    if let Some(about) = about_you {
        out.push_str(
            "What the user has told you about themselves and how they want you to work. \
             This is their own standing description, so prefer it over anything you \
             inferred:\n\n",
        );
        out.push_str(about);
        out.push_str("\n\n");
    }

    if !memories.is_empty() {
        out.push_str(
            "Things you have been asked to remember about this user. \
             Use them when relevant; do not recite them back unprompted.\n",
        );
        for memory in memories.iter().take(MAX_PRELOADED) {
            out.push_str(&format!("- {}\n", memory.content));
        }
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(content: &str, tags: &[&str]) -> MemoryRecord {
        MemoryRecord {
            id: "m1".to_string(),
            content: content.to_string(),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            source: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn lists_memories_with_their_tags() {
        let rendered = format_for_model("deploy", &[memory("deploys run from ops/", &["ops"])]);

        assert!(rendered.contains("- deploys run from ops/"));
        assert!(rendered.contains("[ops]"));
    }

    #[test]
    fn an_empty_result_distinguishes_no_match_from_nothing_stored() {
        assert!(format_for_model("frontend", &[]).contains("frontend"));
        assert!(format_for_model("  ", &[]).contains("Nothing has been remembered yet"));
    }

    #[test]
    fn the_preamble_is_absent_when_there_is_nothing_to_say() {
        assert_eq!(preamble(&[]), None);
    }

    #[test]
    fn the_preamble_tells_the_model_not_to_recite() {
        let text = preamble(&[memory("prefers dark mode", &[])]).expect("preamble");
        assert!(text.contains("prefers dark mode"));
        assert!(text.contains("do not recite"));
    }

    #[test]
    fn the_preamble_is_capped_so_memory_cannot_crowd_out_the_conversation() {
        let many: Vec<MemoryRecord> = (0..40).map(|index| memory(&format!("fact {index}"), &[])).collect();
        let text = preamble(&many).expect("preamble");

        let lines = text.lines().filter(|line| line.starts_with("- ")).count();
        assert_eq!(lines, MAX_PRELOADED);
    }
}
