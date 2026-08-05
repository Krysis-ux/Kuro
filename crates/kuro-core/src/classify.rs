//! What a model's name says about the model.
//!
//! Providers advertise a flat list of ids and nothing else. `GET /v1/models` on
//! NVIDIA NIM answers with sixty-odd strings; on OpenRouter with several
//! hundred. There is no field saying which of them writes code, which reasons,
//! which reads images — and, more urgently, no field saying which of them are
//! not chat models at all.
//!
//! That last point is why this module exists rather than being a nicety. A
//! catalogue like NVIDIA's is not sixty-three chat models: it is chat models
//! mixed with embedding models, rerankers, speech recognisers, image
//! generators and safety classifiers, all in one list. Offering those in a
//! model picker is offering rows that answer every message with an error, and
//! picking one as a fallback — which is what sorting the list and taking the
//! first entry does — is an outage with no explanation attached.
//!
//! So the name is read. It is the only signal on offer, and it is a much better
//! one than it sounds: model ids are named by their publishers to be legible,
//! and `qwen/qwen2.5-coder-32b-instruct` says what it is far more reliably than
//! any heuristic over a description would.
//!
//! ## What this is not
//!
//! It is not a benchmark and it does not claim to rank anything. It answers
//! "what was this trained to do" from the name, and where the name says
//! nothing it says so — [`Speciality::General`] is a real answer here, not a
//! failure. A hand-written table still beats it for the models Kuro has an
//! opinion about, which is why [`crate::free::FreeProvider::models`] still
//! exists and still wins; this fills in the several hundred it does not cover.

use serde::Serialize;

/// What kind of thing a model is, before asking what it is good at.
///
/// The distinction that matters is [`ModelKind::Chat`] versus everything else:
/// only chat models can serve a conversation, and every other variant here is
/// a row that must never reach a model picker or a failover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    /// Takes messages, returns text. The only kind Kuro can talk to.
    Chat,
    /// Turns text into vectors. Answers a chat request with a 400.
    Embedding,
    /// Scores documents against a query.
    Rerank,
    /// Speech recognition or synthesis.
    Speech,
    /// Image or video generation.
    Visual,
    /// A safety classifier. Answers "safe"/"unsafe", not a conversation.
    Guard,
}

impl ModelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::Rerank => "rerank",
            Self::Speech => "speech",
            Self::Visual => "visual",
            Self::Guard => "guard",
        }
    }

    /// Whether this can serve a conversation.
    pub fn is_chat(self) -> bool {
        self == Self::Chat
    }
}

/// What a chat model was trained to be good at.
///
/// Deliberately few. Every extra category is one more thing to be wrong about,
/// and a picker that files models under nine headings is a picker nobody reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Speciality {
    /// Trained on code. The ones worth using in a coding workspace.
    Coding,
    /// Thinks before answering, at the cost of latency.
    Reasoning,
    /// Reads images as well as text.
    Vision,
    /// Small or explicitly named for speed.
    Fast,
    /// The name says nothing more specific. Not a failure — most models.
    General,
}

impl Speciality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Reasoning => "reasoning",
            Self::Vision => "vision",
            Self::Fast => "fast",
            Self::General => "general",
        }
    }

    /// The word the picker puts on the row.
    pub fn label(self) -> &'static str {
        match self {
            Self::Coding => "code",
            Self::Reasoning => "reasoning",
            Self::Vision => "vision",
            Self::Fast => "fast",
            Self::General => "general",
        }
    }
}

/// Everything the name gave up.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Classified {
    pub kind: ModelKind,
    /// Sorted and deduplicated, so two ids that mean the same thing compare
    /// equal. Never empty: a chat model with no signal gets `General`.
    pub specialities: Vec<Speciality>,
    /// Parameter count in billions, when the name states one.
    pub params_b: Option<f32>,
    /// The part before the first slash, when the id is namespaced.
    pub publisher: Option<String>,
}

impl Classified {
    pub fn has(&self, speciality: Speciality) -> bool {
        self.specialities.contains(&speciality)
    }
}

/// Anything at or below this many billion parameters counts as fast.
///
/// Nine rather than eight so the 8B tier lands inside it along with the models
/// that round up to nine, and well below the 12–14B tier where a laptop starts
/// to notice.
const FAST_PARAMS_B: f32 = 9.0;

/// Substrings that mean a model is not a chat model.
///
/// Ordered most-specific first, and checked before anything else, because a
/// name can carry both signals: `nvidia/llama-3.2-nv-rerankqa-1b-v2` contains
/// `llama` and is not a Llama you can talk to.
const NOT_CHAT: &[(&str, ModelKind)] = &[
    // Safety classifiers. Checked before embeddings because `nemoguard` names
    // include a `topic-control` variant that is neither.
    ("guard", ModelKind::Guard),
    ("shield", ModelKind::Guard),
    ("topic-control", ModelKind::Guard),
    ("content-safety", ModelKind::Guard),
    ("jailbreak", ModelKind::Guard),
    // Rerankers before embeddings: `rerankqa` also contains `qa`, and several
    // rerankers are named as a variant of an embedding family.
    ("rerank", ModelKind::Rerank),
    ("embed", ModelKind::Embedding),
    ("retriever", ModelKind::Embedding),
    ("bge-", ModelKind::Embedding),
    ("gte-", ModelKind::Embedding),
    ("nomic-", ModelKind::Embedding),
    ("arctic-", ModelKind::Embedding),
    // Speech, both directions.
    ("whisper", ModelKind::Speech),
    ("parakeet", ModelKind::Speech),
    ("canary", ModelKind::Speech),
    ("fastpitch", ModelKind::Speech),
    ("hifigan", ModelKind::Speech),
    ("magpie-tts", ModelKind::Speech),
    ("-tts", ModelKind::Speech),
    ("-asr", ModelKind::Speech),
    ("speech", ModelKind::Speech),
    // Pixels out rather than in. `stable-diffusion` before any `sd` match so a
    // model merely named `sdxl` is caught by its own entry.
    ("stable-diffusion", ModelKind::Visual),
    ("sdxl", ModelKind::Visual),
    ("flux", ModelKind::Visual),
    ("consistory", ModelKind::Visual),
    ("edify", ModelKind::Visual),
    ("imagen", ModelKind::Visual),
    ("dall-e", ModelKind::Visual),
    ("kandinsky", ModelKind::Visual),
    ("playground-v", ModelKind::Visual),
    ("cosmos", ModelKind::Visual),
    ("-ocr", ModelKind::Visual),
    ("paddleocr", ModelKind::Visual),
    ("table-structure", ModelKind::Visual),
];

/// Substrings meaning "trained on code".
const CODING: &[&str] = &[
    "coder", "codestral", "devstral", "starcoder", "codegemma", "codegeex", "codellama",
    "sqlcoder", "opencoder", "codeqwen", "codex", "swe-", "codium",
    // Bare `code` as a whole word. The sentinel dashes around the name are what
    // make this safe: `-code` matches `north-mini-code` and `granite-code-8b`
    // without also matching `encode` or `unicode`.
    "-code",
];

/// Substrings meaning "reasons before answering".
const REASONING: &[&str] = &[
    "reason", "thinking", "-think", "qwq", "magistral", "-r1", "r1-", "deepthink",
    "marco-o1", "skywork-o1", "phi-4-reasoning", "-o1", "-o3",
];

/// Substrings meaning "reads images".
const VISION: &[&str] = &[
    "vision", "llava", "pixtral", "-vl-", "-vl", "vlm", "internvl", "cogvlm", "molmo",
    "kosmos", "fuyu", "paligemma", "florence", "maverick", "scout", "nvlm", "-vila",
];

/// Substrings a publisher uses to mean "this one is quick".
const FAST: &[&str] = &["mini", "flash", "instant", "nano", "tiny", "lite", "turbo", "-air"];

/// Read a model id.
///
/// Case and separators are normalised first, so `Qwen/Qwen2.5-Coder-32B` and
/// `qwen/qwen2.5_coder_32b` classify identically — providers are not consistent
/// about either, and two spellings of one model must not land in two groups.
pub fn classify(model_id: &str) -> Classified {
    let publisher = model_id
        .split_once('/')
        .map(|(head, _)| head.trim().to_ascii_lowercase())
        .filter(|head| !head.is_empty());

    // Only the part after the publisher is read.
    //
    // Caught on real data: OpenRouter lists `thinkingmachines/inkling`, and
    // matching the whole id tagged it as a reasoning model on the strength of
    // the company's name. A publisher is who made the model, not what it does,
    // and there are enough companies named after a capability — Thinking
    // Machines, Perplexity, Vision — for this to keep happening.
    let stem = model_id.split_once('/').map_or(model_id, |(_, tail)| tail);

    // `:` is a separator too — Ollama writes `gpt-oss:20b` — and dropping it
    // into the same normal form is what lets one pattern list serve both.
    //
    // Wrapped in dashes so that a marker anchored to a word boundary works at
    // the ends of the name as well as inside it: `-o1` has to match `o1`, which
    // is the entire name of a real model, and `-code` has to match a name that
    // ends in it.
    let normal: String = std::iter::once('-')
        .chain(stem.to_ascii_lowercase().chars().map(|character| {
            match character {
                '_' | ':' | ' ' | '/' => '-',
                other => other,
            }
        }))
        .chain(std::iter::once('-'))
        .collect();

    if let Some(kind) = not_chat_kind(&normal) {
        return Classified {
            kind,
            specialities: Vec::new(),
            params_b: params_of(&normal),
            publisher,
        };
    }

    let params_b = params_of(&normal);
    let mut specialities = Vec::new();

    if CODING.iter().any(|marker| normal.contains(marker)) {
        specialities.push(Speciality::Coding);
    }
    if REASONING.iter().any(|marker| normal.contains(marker)) {
        specialities.push(Speciality::Reasoning);
    }
    if VISION.iter().any(|marker| normal.contains(marker)) {
        specialities.push(Speciality::Vision);
    }
    // Size decides before wording does. A vendor calling a 24B model "small"
    // is speaking relative to its own range, not to a laptop.
    let fast_by_size = params_b.is_some_and(|billions| billions <= FAST_PARAMS_B);
    let fast_by_name = params_b.is_none() && FAST.iter().any(|marker| normal.contains(marker));
    if fast_by_size || fast_by_name {
        specialities.push(Speciality::Fast);
    }

    if specialities.is_empty() {
        specialities.push(Speciality::General);
    }
    specialities.sort_unstable();
    specialities.dedup();

    Classified {
        kind: ModelKind::Chat,
        specialities,
        params_b,
        publisher,
    }
}

/// Which non-chat kind this is, if it is one.
fn not_chat_kind(normal: &str) -> Option<ModelKind> {
    NOT_CHAT
        .iter()
        .find(|(marker, _)| normal.contains(marker))
        .map(|(_, kind)| *kind)
}

/// The parameter count a name states, in billions.
///
/// Takes the largest figure in the name rather than the first, so that a
/// mixture-of-experts id naming both its total and its active count —
/// `qwen3-235b-a22b` — is read as the 235B model it is. `8x7b` is multiplied
/// out for the same reason: the memory it needs is the total, not one expert.
fn params_of(normal: &str) -> Option<f32> {
    let bytes = normal.as_bytes();
    let mut best: Option<f32> = None;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != b'b' {
            index += 1;
            continue;
        }
        // A digit may not follow, or `4bit` and `bf16` become parameter counts.
        if bytes
            .get(index + 1)
            .is_some_and(|next| next.is_ascii_alphanumeric())
        {
            index += 1;
            continue;
        }

        let start = number_start(bytes, index);
        if start == index {
            index += 1;
            continue;
        }

        let Ok(digits) = std::str::from_utf8(&bytes[start..index]) else {
            index += 1;
            continue;
        };
        let Ok(mut value) = digits.parse::<f32>() else {
            index += 1;
            continue;
        };

        // `8x7b`: the figure before the `x` multiplies the one after it.
        if start > 0 && bytes[start - 1] == b'x' {
            let experts_start = number_start(bytes, start - 1);
            if let Some(experts) = std::str::from_utf8(&bytes[experts_start..start - 1])
                .ok()
                .and_then(|text| text.parse::<f32>().ok())
            {
                value *= experts;
            }
        }

        best = Some(best.map_or(value, |held: f32| held.max(value)));
        index += 1;
    }

    best.filter(|value| *value > 0.0)
}

/// Where the run of digits ending just before `end` begins.
///
/// A single decimal point is part of the number, so `3.5b` reads as 3.5. A
/// second one ends it, because `llama-3.3-70b` must read as 70 and not as the
/// version number in front of it.
fn number_start(bytes: &[u8], end: usize) -> usize {
    let mut start = end;
    let mut seen_point = false;

    while start > 0 {
        let previous = bytes[start - 1];
        if previous.is_ascii_digit() {
            start -= 1;
        } else if previous == b'.' && !seen_point && start >= 2 && bytes[start - 2].is_ascii_digit()
        {
            seen_point = true;
            start -= 1;
        } else {
            break;
        }
    }

    // A leading `.` is a separator, not part of the figure: `flux.1b` would
    // otherwise read as `.1`.
    if start < end && bytes[start] == b'.' {
        start += 1;
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(id: &str) -> ModelKind {
        classify(id).kind
    }

    fn params(id: &str) -> Option<f32> {
        classify(id).params_b
    }

    #[test]
    fn a_coder_is_recognised_however_its_publisher_spells_it() {
        for id in [
            "qwen/qwen2.5-coder-32b-instruct",
            "Qwen/Qwen2.5-Coder-32B-Instruct",
            "mistralai/codestral-22b-instruct-v0.1",
            "mistralai/devstral-small-2505",
            "bigcode/starcoder2-15b",
            "deepseek-ai/deepseek-coder-6.7b-instruct",
        ] {
            assert!(
                classify(id).has(Speciality::Coding),
                "`{id}` should be a coding model"
            );
        }
    }

    #[test]
    fn a_reasoner_is_recognised() {
        for id in [
            "deepseek-ai/deepseek-r1",
            "qwen/qwq-32b-preview",
            "mistralai/magistral-small-2506",
            "nvidia/llama-3.1-nemotron-ultra-253b-v1",
            "microsoft/phi-4-reasoning-plus",
        ] {
            let classified = classify(id);
            // Nemotron Ultra is here to prove the negative: nothing in that name
            // says "reasoning", so it must not be guessed at.
            if id.contains("nemotron-ultra") {
                assert!(!classified.has(Speciality::Reasoning), "`{id}` was guessed at");
                continue;
            }
            assert!(classified.has(Speciality::Reasoning), "`{id}`");
        }
    }

    #[test]
    fn a_vision_model_is_recognised() {
        for id in [
            "meta/llama-3.2-90b-vision-instruct",
            "qwen/qwen2.5-vl-72b-instruct",
            "mistralai/pixtral-12b",
            "meta/llama-4-maverick-17b-128e-instruct",
            "nvidia/nvlm-d-72b",
        ] {
            assert!(classify(id).has(Speciality::Vision), "`{id}`");
        }
    }

    #[test]
    fn the_things_that_are_not_chat_models_are_kept_out() {
        // The entire reason this module exists. Each of these appears in a real
        // provider catalogue beside the chat models, and each would answer a
        // conversation with an error.
        for (id, expected) in [
            ("nvidia/nv-embedqa-e5-v5", ModelKind::Embedding),
            ("baai/bge-m3", ModelKind::Embedding),
            ("nvidia/llama-3.2-nv-embedqa-1b-v2", ModelKind::Embedding),
            ("nvidia/llama-3.2-nv-rerankqa-1b-v2", ModelKind::Rerank),
            ("meta/llama-guard-4-12b", ModelKind::Guard),
            ("nvidia/llama-3.1-nemoguard-8b-topic-control", ModelKind::Guard),
            ("openai/whisper-large-v3", ModelKind::Speech),
            ("nvidia/parakeet-ctc-1.1b-asr", ModelKind::Speech),
            ("black-forest-labs/flux.1-dev", ModelKind::Visual),
            ("stabilityai/stable-diffusion-3.5-large", ModelKind::Visual),
        ] {
            assert_eq!(kind(id), expected, "`{id}`");
            assert!(!kind(id).is_chat(), "`{id}` must never reach a picker");
        }
    }

    #[test]
    fn ordinary_chat_models_stay_chat_models() {
        for id in [
            "meta/llama-3.3-70b-instruct",
            "mistralai/mistral-large-2-instruct",
            "google/gemma-3-27b-it",
            "openai/gpt-oss-120b",
            "nvidia/llama-3.3-nemotron-super-49b-v1.5",
        ] {
            assert!(kind(id).is_chat(), "`{id}`");
        }
    }

    #[test]
    fn a_llama_that_is_a_reranker_is_not_read_as_a_llama() {
        // Substring order is load-bearing: this id contains `llama`, and the
        // check that matters has to win.
        let classified = classify("nvidia/llama-3.2-nv-rerankqa-1b-v2");
        assert_eq!(classified.kind, ModelKind::Rerank);
        assert!(classified.specialities.is_empty());
    }

    #[test]
    fn parameter_counts_are_read_out_of_the_name() {
        assert_eq!(params("meta/llama-3.3-70b-instruct"), Some(70.0));
        assert_eq!(params("meta/llama-3.1-8b-instruct"), Some(8.0));
        assert_eq!(params("deepseek-ai/deepseek-coder-6.7b-instruct"), Some(6.7));
        assert_eq!(params("openai/gpt-oss-120b"), Some(120.0));
        assert_eq!(params("google/gemma-3-27b-it"), Some(27.0));
    }

    #[test]
    fn a_version_number_is_not_a_parameter_count() {
        // `llama-3.3-70b` must read as 70, never as 3.3.
        assert_eq!(params("meta/llama-3.3-70b-instruct"), Some(70.0));
        assert_eq!(params("mistralai/mistral-nemo-12b-instruct"), Some(12.0));
        // Nothing in these states a size, and inventing one would put them in
        // the wrong speed bucket.
        assert_eq!(params("microsoft/phi-4"), None);
        assert_eq!(params("mistralai/mistral-large-2-instruct"), None);
    }

    #[test]
    fn a_quantisation_suffix_is_not_a_parameter_count() {
        // `4bit` and `bf16` both put a `b` next to digits.
        assert_eq!(params("some/model-7b-4bit"), Some(7.0));
        assert_eq!(params("some/model-bf16"), None);
    }

    #[test]
    fn a_mixture_of_experts_reports_what_it_actually_costs_to_run() {
        // Both figures appear in the name; the larger is the one that decides
        // whether this fits anywhere.
        assert_eq!(params("qwen/qwen3-235b-a22b"), Some(235.0));
        assert_eq!(params("qwen/qwen3-30b-a3b"), Some(30.0));
        // And an `8x7b` is 56B of weights, not 7.
        assert_eq!(params("mistralai/mixtral-8x7b-instruct-v0.1"), Some(56.0));
    }

    #[test]
    fn small_models_are_fast_and_large_ones_are_not() {
        assert!(classify("meta/llama-3.1-8b-instruct").has(Speciality::Fast));
        assert!(classify("qwen/qwen2.5-3b-instruct").has(Speciality::Fast));
        assert!(!classify("meta/llama-3.3-70b-instruct").has(Speciality::Fast));

        // A stated size overrules the vendor's own adjective: Mistral Small is
        // 24B, which is not fast on anybody's laptop.
        assert!(!classify("mistralai/mistral-small-24b-instruct").has(Speciality::Fast));
        // With no size to go on, the adjective is all there is.
        assert!(classify("google/gemini-flash").has(Speciality::Fast));
    }

    #[test]
    fn a_model_can_be_more_than_one_thing() {
        let classified = classify("qwen/qwen2.5-vl-7b-instruct");
        assert!(classified.has(Speciality::Vision));
        assert!(classified.has(Speciality::Fast));
        assert!(!classified.has(Speciality::General));
    }

    #[test]
    fn a_name_that_says_nothing_says_general_rather_than_nothing() {
        // An empty speciality list would render as a row with no tag at all,
        // which reads as a bug rather than as an ordinary model.
        let classified = classify("mistralai/mistral-large-2-instruct");
        assert_eq!(classified.specialities, vec![Speciality::General]);
    }

    #[test]
    fn a_publisher_named_after_a_capability_does_not_lend_it_to_the_model() {
        // Found on real OpenRouter data: `thinkingmachines/inkling` was tagged
        // as a reasoning model because of the company that made it. There are
        // enough firms named after a capability for this to matter.
        let classified = classify("thinkingmachines/inkling");
        assert!(!classified.has(Speciality::Reasoning), "the company thinks; the model may not");
        assert_eq!(classified.specialities, vec![Speciality::General]);

        // And the reverse still works: a model whose own name says it.
        assert!(classify("thinkingmachines/some-thinking-model").has(Speciality::Reasoning));
    }

    #[test]
    fn a_marker_at_either_end_of_the_name_still_matches() {
        // `o1` is the whole name, with no separator in front of it to anchor to.
        assert!(classify("openai/o1").has(Speciality::Reasoning));
        assert!(classify("openai/o3-mini").has(Speciality::Reasoning));
        // And `code` as the last word, which a bare `coder` check misses.
        assert!(classify("cohere/north-mini-code").has(Speciality::Coding));
        assert!(classify("ibm/granite-code-8b").has(Speciality::Coding));
    }

    #[test]
    fn a_word_merely_containing_a_marker_does_not_match_it() {
        // The sentinel dashes are what make the bare `-code` entry safe.
        assert!(!classify("some/encode-benchmark").has(Speciality::Coding));
        assert!(!classify("some/unicode-tokeniser").has(Speciality::Coding));
    }

    #[test]
    fn the_publisher_is_taken_from_the_namespace_when_there_is_one() {
        assert_eq!(classify("meta/llama-3.3-70b").publisher.as_deref(), Some("meta"));
        assert_eq!(classify("Qwen/Qwen3-32B").publisher.as_deref(), Some("qwen"));
        assert_eq!(classify("llama-3.3-70b").publisher, None);
    }

    #[test]
    fn separators_do_not_change_the_answer() {
        // Ollama writes `gpt-oss:20b`, Hugging Face writes `openai/gpt-oss-20b`.
        // Two spellings of one model must not land in two groups.
        let colon = classify("gpt-oss:20b");
        let dash = classify("gpt-oss-20b");
        assert_eq!(colon.specialities, dash.specialities);
        assert_eq!(colon.params_b, dash.params_b);
    }

    #[test]
    fn specialities_are_sorted_so_two_equal_models_compare_equal() {
        let classified = classify("qwen/qwen2.5-coder-7b-instruct");
        let mut sorted = classified.specialities.clone();
        sorted.sort_unstable();
        assert_eq!(classified.specialities, sorted);
    }
}
