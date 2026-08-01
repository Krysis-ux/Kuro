//! Kuro's built-in "recommended models" list.
//!
//! This is static data rather than database rows so that a new release can
//! refresh the recommendations without a migration. Only the repository and the
//! preferred quantization are pinned — the exact `.gguf` filename is resolved
//! against the Hugging Face API at pull time, so these entries keep working
//! when a repository renames or re-uploads its files.

/// What a model is worth choosing *for*.
///
/// The list without this was a ramp by size, which answers "what will run on my
/// machine" and not "which one should I use". Those are different questions, and
/// the second is the one somebody has when they open the models screen: a 7B
/// coder and a 7B general model have the same fit estimate and are not remotely
/// interchangeable for the thing being attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Writing and changing code. What the Code page should suggest.
    Coding,
    /// Reading images.
    Vision,
    /// Transcribing or understanding speech.
    Audio,
    /// Ordinary conversation and writing.
    Text,
    /// Decent at everything, best at nothing. The safe single choice.
    AllRound,
}

impl Purpose {
    pub const ALL: &'static [Purpose] =
        &[Self::AllRound, Self::Coding, Self::Vision, Self::Audio, Self::Text];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Vision => "vision",
            Self::Audio => "audio",
            Self::Text => "text",
            Self::AllRound => "all_round",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Coding => "For coding",
            Self::Vision => "For images",
            Self::Audio => "For audio",
            Self::Text => "For writing and chat",
            Self::AllRound => "Good at everything",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Self::Coding => {
                "Trained on code. These are the ones to use in a coding workspace, where the \
                 model has to read a real project and change it."
            }
            Self::Vision => "Can look at an image you attach and describe or reason about it.",
            Self::Audio => "Can take audio as input rather than only text.",
            Self::Text => "Conversation, explanation and writing.",
            Self::AllRound => {
                "If you are only going to install one, install one of these. Competent at \
                 code, chat and reasoning without being the best at any of them."
            }
        }
    }
}

/// A model Kuro suggests, before the user has downloaded anything.
#[derive(Debug, Clone, Copy)]
pub struct CuratedModel {
    /// Short handle used by the CLI, e.g. `kuro pull qwen3-4b`.
    pub slug: &'static str,
    pub display_name: &'static str,
    pub hf_repo: &'static str,
    pub default_quant: &'static str,
    pub quants: &'static [&'static str],
    pub param_count: &'static str,
    pub family: &'static str,
    pub capabilities: &'static [&'static str],
    /// What this model is worth choosing for. Ordered best-first, so the primary
    /// purpose is the one the recommendation screen groups it under.
    pub purposes: &'static [Purpose],
    pub context_length: u32,
    /// Approximate download size of the default quantization. Used only to show
    /// a fit estimate before the real size is known from the repository.
    pub approx_size_bytes: u64,
    pub blurb: &'static str,
}

const GB: u64 = 1024 * 1024 * 1024;
const MB: u64 = 1024 * 1024;

/// The recommended set, ordered smallest first so the list reads as a ramp.
pub const CURATED_MODELS: &[CuratedModel] = &[
    CuratedModel {
        slug: "qwen2.5-0.5b",
        display_name: "Qwen2.5 0.5B Instruct",
        hf_repo: "Qwen/Qwen2.5-0.5B-Instruct-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "0.5B",
        family: "qwen2.5",
        capabilities: &["tools"],
        purposes: &[Purpose::Text],
        context_length: 32768,
        approx_size_bytes: 400 * MB,
        blurb: "Tiny and fast. Good for checking that everything works.",
    },
    CuratedModel {
        slug: "llama3.2-1b",
        display_name: "Llama 3.2 1B Instruct",
        hf_repo: "unsloth/Llama-3.2-1B-Instruct-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "1B",
        family: "llama3.2",
        capabilities: &["tools"],
        purposes: &[Purpose::Text],
        context_length: 131072,
        approx_size_bytes: 800 * MB,
        blurb: "Very light. Runs comfortably on any Mac.",
    },
    CuratedModel {
        slug: "llama3.2-3b",
        display_name: "Llama 3.2 3B Instruct",
        hf_repo: "unsloth/Llama-3.2-3B-Instruct-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "3B",
        family: "llama3.2",
        capabilities: &["tools"],
        purposes: &[Purpose::AllRound, Purpose::Text],
        context_length: 131072,
        approx_size_bytes: 2 * GB,
        blurb: "A good everyday default on modest hardware.",
    },
    CuratedModel {
        slug: "qwen3-4b",
        display_name: "Qwen3 4B Instruct",
        hf_repo: "unsloth/Qwen3-4B-Instruct-2507-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0"],
        param_count: "4B",
        family: "qwen3",
        capabilities: &["tools", "reasoning"],
        purposes: &[Purpose::AllRound, Purpose::Coding],
        context_length: 262144,
        approx_size_bytes: 2500 * MB,
        blurb: "Strong reasoning for its size, with a very long context.",
    },
    CuratedModel {
        slug: "gemma3-4b",
        display_name: "Gemma 3 4B Instruct",
        hf_repo: "unsloth/gemma-3-4b-it-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "4B",
        family: "gemma3",
        capabilities: &["vision"],
        purposes: &[Purpose::Vision, Purpose::Text],
        context_length: 131072,
        approx_size_bytes: 3 * GB,
        blurb: "Handles images as well as text.",
    },
    CuratedModel {
        slug: "mistral-7b",
        display_name: "Mistral 7B Instruct v0.3",
        hf_repo: "bartowski/Mistral-7B-Instruct-v0.3-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "7B",
        family: "mistral",
        capabilities: &["tools"],
        purposes: &[Purpose::AllRound, Purpose::Text],
        context_length: 32768,
        approx_size_bytes: 4400 * MB,
        blurb: "A dependable general-purpose model.",
    },
    CuratedModel {
        slug: "qwen2.5-coder-7b",
        display_name: "Qwen2.5 Coder 7B Instruct",
        hf_repo: "unsloth/Qwen2.5-Coder-7B-Instruct-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "7B",
        family: "qwen2.5-coder",
        capabilities: &["tools"],
        purposes: &[Purpose::Coding],
        context_length: 32768,
        approx_size_bytes: 4700 * MB,
        blurb: "Tuned for writing and explaining code.",
    },
    CuratedModel {
        slug: "llama3.1-8b",
        display_name: "Llama 3.1 8B Instruct",
        hf_repo: "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "8B",
        family: "llama3.1",
        capabilities: &["tools"],
        purposes: &[Purpose::AllRound, Purpose::Text],
        context_length: 131072,
        approx_size_bytes: 4900 * MB,
        blurb: "A well-rounded model if you have the memory for it.",
    },
    CuratedModel {
        slug: "qwen2.5-coder-1.5b",
        display_name: "Qwen2.5 Coder 1.5B Instruct",
        hf_repo: "Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "1.5B",
        family: "qwen2.5-coder",
        capabilities: &["tools"],
        purposes: &[Purpose::Coding],
        context_length: 32768,
        approx_size_bytes: 1100 * MB,
        blurb: "The smallest model here worth pointing at real code.",
    },
    CuratedModel {
        slug: "qwen2.5-coder-3b",
        display_name: "Qwen2.5 Coder 3B Instruct",
        hf_repo: "Qwen/Qwen2.5-Coder-3B-Instruct-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "3B",
        family: "qwen2.5-coder",
        capabilities: &["tools"],
        purposes: &[Purpose::Coding],
        context_length: 32768,
        approx_size_bytes: 2 * GB,
        blurb: "Writes usable code on a laptop that cannot hold the 7B.",
    },
    CuratedModel {
        slug: "qwen3-coder-30b",
        display_name: "Qwen3 Coder 30B A3B",
        hf_repo: "unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0"],
        param_count: "30B",
        family: "qwen3-coder",
        capabilities: &["tools", "reasoning"],
        purposes: &[Purpose::Coding],
        context_length: 262144,
        // Mixture-of-experts: 30B of weights on disk, but only about 3B active
        // per token, so it runs far faster than its size suggests. The fit
        // estimate is still driven by the file, which has to be resident.
        approx_size_bytes: 18 * GB,
        blurb: "The best coding model here, if you have 32GB. Large on disk, fast to run.",
    },
    CuratedModel {
        slug: "devstral-24b",
        display_name: "Devstral Small 2507",
        hf_repo: "unsloth/Devstral-Small-2507-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "24B",
        family: "devstral",
        capabilities: &["tools"],
        purposes: &[Purpose::Coding],
        context_length: 131072,
        approx_size_bytes: 14 * GB,
        blurb: "Built for agentic coding — using tools across a whole repository, not \
                one-file completions.",
    },
    CuratedModel {
        slug: "qwen3-8b",
        display_name: "Qwen3 8B",
        hf_repo: "unsloth/Qwen3-8B-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0"],
        param_count: "8B",
        family: "qwen3",
        capabilities: &["tools", "reasoning"],
        purposes: &[Purpose::AllRound, Purpose::Coding, Purpose::Text],
        context_length: 131072,
        approx_size_bytes: 5100 * MB,
        blurb: "Thinks before answering, and is good at both code and conversation.",
    },
    CuratedModel {
        slug: "qwen2.5-vl-7b",
        display_name: "Qwen2.5-VL 7B Instruct",
        hf_repo: "unsloth/Qwen2.5-VL-7B-Instruct-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "7B",
        family: "qwen2.5-vl",
        capabilities: &["vision", "tools"],
        purposes: &[Purpose::Vision],
        context_length: 32768,
        approx_size_bytes: 5 * GB,
        blurb: "Reads screenshots, diagrams and documents, not just photographs.",
    },
    CuratedModel {
        slug: "gemma3-12b",
        display_name: "Gemma 3 12B Instruct",
        hf_repo: "unsloth/gemma-3-12b-it-GGUF",
        default_quant: "Q4_K_M",
        quants: &["Q4_K_M", "Q5_K_M", "Q8_0"],
        param_count: "12B",
        family: "gemma3",
        capabilities: &["vision"],
        purposes: &[Purpose::Vision, Purpose::AllRound],
        context_length: 131072,
        approx_size_bytes: 8 * GB,
        blurb: "The larger vision model. Noticeably better on dense images.",
    },
    CuratedModel {
        slug: "whisper-base",
        display_name: "Whisper Base",
        hf_repo: "ggerganov/whisper.cpp",
        default_quant: "Q5_1",
        quants: &["Q5_1", "Q8_0"],
        param_count: "74M",
        family: "whisper",
        capabilities: &["audio"],
        purposes: &[Purpose::Audio],
        context_length: 448,
        approx_size_bytes: 60 * MB,
        blurb: "Speech to text, in most languages. Tiny, and runs on anything.",
    },
    CuratedModel {
        slug: "whisper-large-v3-turbo",
        display_name: "Whisper Large v3 Turbo",
        hf_repo: "ggerganov/whisper.cpp",
        default_quant: "Q5_0",
        quants: &["Q5_0", "Q8_0"],
        param_count: "809M",
        family: "whisper",
        capabilities: &["audio"],
        purposes: &[Purpose::Audio],
        context_length: 448,
        approx_size_bytes: 574 * MB,
        blurb: "Much more accurate transcription, still fast enough for real use.",
    },
];

/// Models worth choosing for one purpose, smallest first.
///
/// A model appears under every purpose it lists, not only its first: Qwen3 8B is
/// a genuine answer to both "what should I code with" and "what should I install
/// if I install one thing", and hiding it from one of those lists to keep the
/// grouping tidy would be tidiness at the user's expense.
pub fn for_purpose(purpose: Purpose) -> Vec<&'static CuratedModel> {
    CURATED_MODELS
        .iter()
        .filter(|model| model.purposes.contains(&purpose))
        .collect()
}

/// The purpose a model is listed under first, for a one-word label on a card.
pub fn primary_purpose(model: &CuratedModel) -> Purpose {
    model.purposes.first().copied().unwrap_or(Purpose::Text)
}

pub fn find_curated(slug: &str) -> Option<&'static CuratedModel> {
    let needle = slug.to_ascii_lowercase();
    CURATED_MODELS
        .iter()
        .find(|model| model.slug.eq_ignore_ascii_case(&needle))
}

impl CuratedModel {
    /// Canonical model id, e.g. `qwen3-4b:q4_k_m`.
    ///
    /// The quantization is part of the id so two quantizations of the same
    /// model can coexist locally.
    pub fn model_id(&self, quant: &str) -> String {
        format!("{}:{}", self.slug, quant.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn slugs_are_unique_and_lowercase() {
        let mut seen = HashSet::new();
        for model in CURATED_MODELS {
            assert!(seen.insert(model.slug), "duplicate slug: {}", model.slug);
            assert_eq!(
                model.slug,
                model.slug.to_ascii_lowercase(),
                "slugs are typed on the command line and must be lowercase"
            );
        }
    }

    #[test]
    fn every_model_offers_its_default_quantization() {
        for model in CURATED_MODELS {
            assert!(
                model.quants.contains(&model.default_quant),
                "{} defaults to a quantization it does not list",
                model.slug
            );
            assert!(!model.hf_repo.is_empty());
            assert!(model.approx_size_bytes > 0);
        }
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert!(find_curated("QWEN3-4B").is_some());
        assert!(find_curated("nope").is_none());
    }

    #[test]
    fn model_id_is_stable_and_lowercase() {
        let model = find_curated("qwen3-4b").expect("present");
        assert_eq!(model.model_id("Q4_K_M"), "qwen3-4b:q4_k_m");
    }

    #[test]
    fn every_purpose_has_something_to_recommend() {
        // A heading with nothing under it reads as a broken screen, and the
        // Code page's "recommended for coding" list has to be non-empty or the
        // suggestion is worse than no suggestion.
        for purpose in Purpose::ALL {
            assert!(
                !for_purpose(*purpose).is_empty(),
                "`{}` would be an empty section",
                purpose.as_str()
            );
        }
    }

    #[test]
    fn every_model_says_what_it_is_for() {
        for model in CURATED_MODELS {
            assert!(
                !model.purposes.is_empty(),
                "`{}` would not appear under any heading",
                model.slug
            );
        }
    }

    #[test]
    fn a_purpose_matches_the_capability_that_makes_it_possible() {
        // A model listed for images that cannot see one is a recommendation that
        // fails the moment somebody attaches a photograph.
        for model in for_purpose(Purpose::Vision) {
            assert!(
                model.capabilities.contains(&"vision"),
                "`{}` is listed for images without the vision capability",
                model.slug
            );
        }
        for model in for_purpose(Purpose::Audio) {
            assert!(
                model.capabilities.contains(&"audio"),
                "`{}` is listed for audio without the audio capability",
                model.slug
            );
        }
    }

    #[test]
    fn coding_recommendations_span_small_and_large_machines() {
        let coding = for_purpose(Purpose::Coding);
        assert!(coding.len() >= 3, "one option is not a recommendation");

        let smallest = coding.iter().map(|model| model.approx_size_bytes).min().unwrap();
        assert!(
            smallest < 2 * GB,
            "somebody on a small laptop needs an answer too"
        );
    }

    #[test]
    fn a_model_can_be_recommended_for_more_than_one_thing() {
        let qwen3 = find_curated("qwen3-8b").expect("present");
        assert!(qwen3.purposes.contains(&Purpose::Coding));
        assert!(qwen3.purposes.contains(&Purpose::AllRound));
        assert_eq!(primary_purpose(qwen3), Purpose::AllRound);
    }

    #[test]
    fn every_purpose_explains_itself_for_the_heading() {
        for purpose in Purpose::ALL {
            assert!(!purpose.label().is_empty());
            assert!(purpose.blurb().len() > 30, "`{}` needs a real description", purpose.as_str());
        }
    }
}
