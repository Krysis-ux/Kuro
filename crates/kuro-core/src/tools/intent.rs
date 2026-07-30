//! Deciding whether a message needs the web, and what to search for.
//!
//! The web switch used to mean "search this message, verbatim, always". That is
//! wrong in two ways that were both visible in real transcripts:
//!
//! * "hi" was searched. Five dictionary definitions of the word "hi" came back,
//!   none of which answer a greeting, and the model — told to use the results and
//!   to admit when it does not know — replied "I don't know" and listed them.
//! * "search for more research papers on deepseek and llms and how we can
//!   optimize them" was searched as that whole sentence. Search engines rank on
//!   the query they are given, so the instruction words competed with the actual
//!   subject.
//!
//! So two decisions live here. Whether to search at all, which is a
//! classification, and what string to send if so, which is an extraction. Both
//! are deliberately blunt keyword work rather than a model call: this runs before
//! the first token of every message with the switch on, and spending an inference
//! round on it would be felt as latency on every turn.
//!
//! The bias is towards searching. A needless search costs a second and some
//! context; a missed one costs the answer. Skipping only happens on a short list
//! of cases where searching is provably useless.

/// What to do about the web for one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchDecision {
    /// Search, using this query rather than the raw message.
    Search(String),
    /// Do not search. The reason is shown in the log and in the request inspector.
    Skip(SkipReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// A greeting or pleasantry. There is nothing to look up.
    Conversational,
    /// A question about Kuro, this machine, or the model's own setup. The answer
    /// is in the system prompt; the web knows nothing about this deployment.
    AboutThisSetup,
    /// A question about what has already been said in this conversation.
    AboutThisConversation,
    /// Too short to be a query.
    TooShort,
}

impl SkipReason {
    /// Why no search ran, phrased for a person reading the inspector.
    pub fn explain(self) -> &'static str {
        match self {
            Self::Conversational => "no search — nothing to look up",
            Self::AboutThisSetup => "no search — this is about Kuro, not the web",
            Self::AboutThisConversation => "no search — the answer is in this conversation",
            Self::TooShort => "no search — the message is too short to be a query",
        }
    }
}

/// Greetings and pleasantries, matched against the whole message.
///
/// Matching the whole message rather than looking for these as substrings is the
/// point: "hi" is a greeting, "hi-fi speaker reviews" is a query.
const PLEASANTRIES: &[&str] = &[
    "hi", "hii", "hiya", "hey", "heya", "hello", "helo", "yo", "sup", "wassup", "whats up",
    "what's up", "howdy", "greetings", "good morning", "good afternoon", "good evening",
    "good night", "morning", "gm", "gn", "thanks", "thank you", "thanks a lot", "thank you so much",
    "ty", "thx", "cheers", "ok", "okay", "k", "kk", "got it", "gotcha", "cool", "nice", "great",
    "awesome", "perfect", "lol", "lmao", "haha", "yes", "yeah", "yep", "yup", "no", "nope", "nah",
    "sure", "please", "pls", "np", "no problem", "youre welcome", "you're welcome", "bye",
    "goodbye", "see you", "see ya", "later", "good bot", "nevermind", "never mind", "test",
    "testing", "ping", "hello there", "how are you", "how are you doing", "hows it going",
    "how's it going", "how do you do", "whats good", "what's good",
];

/// Phrases that mean the question is about this application, this machine, or
/// the model's own configuration.
///
/// Deliberately specific. A loose rule here would swallow real questions — "can
/// you read this paper for me" is a fetch, not a question about capabilities.
const ABOUT_THIS_SETUP: &[&str] = &[
    "mcp server",
    "mcp servers",
    "mcp connection",
    "what mcp",
    "which mcp",
    "my file system",
    "my filesystem",
    "my files",
    "my folder",
    "my computer",
    "my machine",
    "my laptop",
    "my desktop",
    "this machine",
    "this computer",
    "do you have access",
    "do you have internet",
    "do you have web",
    "can you access my",
    "can you see my",
    "what tools do you",
    "which tools do you",
    "what tools are",
    "what can you do",
    "what are you able",
    "what model are you",
    "which model are you",
    "who are you",
    "what are you running",
    "are you running locally",
    "your capabilities",
    "your tools",
    "your memory",
    "what skills",
    "which skills",
    "are you connected",
    "am i connected",
    "did i connect",
    "did i just connect",
    "kuro",
];

/// Phrases about the conversation itself.
const ABOUT_THIS_CONVERSATION: &[&str] = &[
    "what did i just say",
    "what did i say",
    "what did i ask",
    "what was my first",
    "what was my last",
    "what did you just say",
    "what did you say",
    "repeat that",
    "say that again",
    "summarise this chat",
    "summarize this chat",
    "summarise our conversation",
    "summarize our conversation",
    "earlier in this chat",
    "earlier you said",
];

/// Instruction prefixes stripped before searching.
///
/// Longest first, so "do a search for" is removed whole rather than leaving
/// "a search for" behind after "do " matches.
const LEAD_INS: &[&str] = &[
    "can you please search the web for",
    "can you please search for",
    "could you please search for",
    "i would like you to search for",
    "i want you to search for",
    "i need you to search for",
    "please search the web for",
    "please look up",
    "do a web search for",
    "do a search for",
    "run a search for",
    "can you search for",
    "can you search",
    "could you search for",
    "could you search",
    "can you look up",
    "can you find",
    "can you tell me about",
    "could you tell me about",
    "search the web for",
    "search online for",
    "search google for",
    "look on the web for",
    "please search for",
    "please find",
    "please tell me",
    "search for me",
    "search for",
    "search up",
    "google for",
    "look up",
    "lookup",
    "look for",
    "find me",
    "find out about",
    "find out",
    "tell me about",
    "tell me",
    "what do you know about",
    "give me",
    "show me",
    "research",
    "web search",
    "search",
    "google",
    "please",
    "can you",
    "could you",
];

/// Trailing filler, removed because it never helps a search engine.
const TRAILING_FILLER: &[&str] = &[
    "for me please",
    "for me",
    "please",
    "thanks",
    "thank you",
    "if you can",
    "if possible",
];

/// A query longer than this is trimmed. Search engines weight the head of a
/// query and a rambling tail mostly adds noise.
const MAX_QUERY_CHARS: usize = 200;

/// Decide what the web should do about one message.
pub fn decide(message: &str) -> SearchDecision {
    let normalised = normalise(message);

    // Checked before the length floor, because the shortest messages of all —
    // "hi", "ok", "no" — are exactly the ones this catches.
    //
    // A pleasantry is the whole message or nothing. "hi" skips; "hi, what is the
    // population of Tokyo" is a real question that happens to open politely.
    if PLEASANTRIES.contains(&normalised.as_str()) {
        return SearchDecision::Skip(SkipReason::Conversational);
    }

    if normalised.chars().count() < 3 {
        return SearchDecision::Skip(SkipReason::TooShort);
    }

    // A greeting followed by a question is the question.
    if let Some(rest) = strip_leading_greeting(&normalised) {
        if !rest.is_empty() {
            return decide(&rest);
        }
        return SearchDecision::Skip(SkipReason::Conversational);
    }

    if contains_any(&normalised, ABOUT_THIS_CONVERSATION) {
        return SearchDecision::Skip(SkipReason::AboutThisConversation);
    }

    if contains_any(&normalised, ABOUT_THIS_SETUP) {
        return SearchDecision::Skip(SkipReason::AboutThisSetup);
    }

    SearchDecision::Search(search_query(message))
}

/// Turn a message into the string to hand a search engine.
///
/// The instruction is removed and the subject kept, so "search for research
/// papers on deepseek" is searched as "research papers on deepseek".
pub fn search_query(message: &str) -> String {
    let mut working = collapse_whitespace(message.trim());

    // Strip repeatedly: "can you please search for X" is three prefixes deep.
    for _ in 0..4 {
        let lowered = working.to_ascii_lowercase();
        let Some(matched) = LEAD_INS
            .iter()
            .find(|lead| starts_with_word(&lowered, lead))
        else {
            break;
        };
        let rest = working[matched.len()..].trim_start_matches([' ', ',', ':']).trim();
        if rest.is_empty() {
            // The whole message was an instruction with no subject. Keep what
            // there was rather than searching for nothing.
            break;
        }
        working = rest.to_string();
    }

    for filler in TRAILING_FILLER {
        let lowered = working.to_ascii_lowercase();
        let trimmed = lowered.trim_end_matches(['.', '!', '?', ' ']);
        if let Some(head) = trimmed.strip_suffix(filler) {
            if head.trim_end_matches([' ', ',']).len() > 3 {
                working = working[..head.trim_end_matches([' ', ',']).len()].to_string();
            }
        }
    }

    let working = working.trim_matches(|c: char| c.is_whitespace() || c == ',').to_string();

    if working.is_empty() {
        return collapse_whitespace(message.trim());
    }

    truncate_on_word(&working, MAX_QUERY_CHARS)
}

/// Lowercase, drop punctuation that does not change meaning, collapse spaces.
fn normalise(message: &str) -> String {
    let lowered = message.trim().to_ascii_lowercase();
    let stripped: String = lowered
        .chars()
        .filter(|c| !matches!(c, '.' | '!' | '?' | ',' | ';' | ':' | '"' | '*' | '~'))
        .collect();
    collapse_whitespace(stripped.trim())
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Remove an opening greeting, returning what follows.
fn strip_leading_greeting(normalised: &str) -> Option<String> {
    const OPENERS: &[&str] = &[
        "hello there", "good morning", "good afternoon", "good evening", "hey there", "hi there",
        "hello", "hiya", "hey", "hi", "yo", "howdy",
    ];

    let matched = OPENERS.iter().find(|opener| starts_with_word(normalised, opener))?;
    Some(
        normalised[matched.len()..]
            .trim_start_matches([' ', ',', '-'])
            .trim()
            .to_string(),
    )
}

/// Whether `text` starts with `prefix` at a word boundary.
///
/// Without the boundary check, "search" would match the start of "searching for
/// a job in Berlin" and leave "ing for a job in Berlin".
fn starts_with_word(text: &str, prefix: &str) -> bool {
    let Some(rest) = text.strip_prefix(prefix) else {
        return false;
    };
    rest.is_empty() || rest.starts_with(|c: char| !c.is_alphanumeric())
}

fn contains_any(normalised: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| normalised.contains(needle))
}

fn truncate_on_word(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit).collect();
    match kept.rfind(' ') {
        Some(boundary) if boundary > limit / 2 => kept[..boundary].to_string(),
        _ => kept,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skipped(message: &str) -> SkipReason {
        match decide(message) {
            SearchDecision::Skip(reason) => reason,
            SearchDecision::Search(query) => {
                panic!("`{message}` should not be searched, but produced `{query}`")
            }
        }
    }

    fn searched(message: &str) -> String {
        match decide(message) {
            SearchDecision::Search(query) => query,
            SearchDecision::Skip(reason) => {
                panic!("`{message}` should be searched, but was skipped: {reason:?}")
            }
        }
    }

    #[test]
    fn a_greeting_is_not_a_search() {
        // The bug this module exists for: "hi" returned dictionary definitions
        // of the word "hi", and the model answered "I don't know".
        for greeting in ["hi", "Hi", "hello", "hey!", "  hi  ", "yo", "Good morning"] {
            assert_eq!(skipped(greeting), SkipReason::Conversational, "{greeting}");
        }
    }

    #[test]
    fn thanks_and_acknowledgements_are_not_searches() {
        for message in ["thanks", "thank you", "ok", "got it", "cool", "nice", "lol"] {
            assert_eq!(skipped(message), SkipReason::Conversational, "{message}");
        }
    }

    #[test]
    fn a_greeting_with_a_real_question_after_it_is_still_searched() {
        let query = searched("hi, what is the population of Tokyo");
        assert!(query.contains("population"), "got `{query}`");
        assert!(!query.starts_with("hi"), "the greeting should be gone: `{query}`");
    }

    #[test]
    fn a_word_that_merely_starts_with_a_greeting_is_a_real_query() {
        // "hifi", "hindsight" and "yoga" all begin with a pleasantry's letters.
        assert!(!searched("hifi speaker reviews 2026").is_empty());
        assert!(!searched("yoga classes near me").is_empty());
        assert!(!searched("hindsight bias in trading").is_empty());
    }

    #[test]
    fn questions_about_this_setup_are_answered_from_the_prompt_not_the_web() {
        // Searching the web for "what MCP servers did I connect" cannot work:
        // the web does not know what is on this machine.
        for message in [
            "what mcp servers did i just connect",
            "do you have access to my file system",
            "what tools do you have",
            "what model are you",
            "can you see my files",
            "what can you do",
        ] {
            assert_eq!(skipped(message), SkipReason::AboutThisSetup, "{message}");
        }
    }

    #[test]
    fn questions_about_the_conversation_are_not_searched() {
        assert_eq!(
            skipped("what did i just say"),
            SkipReason::AboutThisConversation
        );
        assert_eq!(
            skipped("summarise our conversation"),
            SkipReason::AboutThisConversation
        );
    }

    #[test]
    fn an_ordinary_question_is_searched() {
        assert!(!searched("who won the 2026 world cup").is_empty());
        assert!(!searched("latest news on interest rates").is_empty());
        assert!(!searched("how does a transformer attention head work").is_empty());
    }

    #[test]
    fn the_instruction_is_stripped_and_the_subject_kept() {
        // The second transcript bug: the whole sentence was the query, so the
        // instruction words competed with the subject for ranking.
        let query = search_query(
            "search for more research papers on deepseek ai and llm's and how we can optimize them",
        );

        assert!(query.starts_with("more research papers"), "got `{query}`");
        assert!(!query.contains("search for"), "got `{query}`");
    }

    #[test]
    fn stacked_polite_prefixes_are_all_removed() {
        assert_eq!(search_query("can you please search for rust lifetimes"), "rust lifetimes");
        assert_eq!(search_query("please look up the ISS orbit"), "the ISS orbit");
        assert_eq!(search_query("google rust async book"), "rust async book");
    }

    #[test]
    fn a_prefix_that_is_only_the_start_of_a_word_is_left_alone() {
        // "searching" must not be stripped down to "ing".
        assert_eq!(search_query("searching for a job in Berlin"), "searching for a job in Berlin");
        assert_eq!(search_query("googling best practices"), "googling best practices");
    }

    #[test]
    fn an_instruction_with_no_subject_keeps_something_to_search() {
        assert!(!search_query("search").is_empty());
        assert!(!search_query("look up").is_empty());
    }

    #[test]
    fn trailing_filler_is_dropped() {
        assert_eq!(search_query("rust async runtimes please"), "rust async runtimes");
        assert_eq!(search_query("find me a good pasta recipe for me"), "a good pasta recipe");
    }

    #[test]
    fn a_very_long_message_is_trimmed_on_a_word_boundary() {
        let long = format!("the history of {}", "cartography ".repeat(60));
        let query = search_query(&long);

        assert!(query.chars().count() <= MAX_QUERY_CHARS);
        assert!(!query.ends_with("cartograp"), "should cut on a word: `{query}`");
    }

    #[test]
    fn a_message_too_short_to_be_a_query_is_skipped() {
        assert_eq!(skipped("a"), SkipReason::TooShort);
        assert_eq!(skipped("?"), SkipReason::TooShort);
    }

    #[test]
    fn every_skip_reason_can_explain_itself() {
        for reason in [
            SkipReason::Conversational,
            SkipReason::AboutThisSetup,
            SkipReason::AboutThisConversation,
            SkipReason::TooShort,
        ] {
            assert!(reason.explain().starts_with("no search"));
        }
    }
}
