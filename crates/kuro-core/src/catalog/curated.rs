//! Kuro's built-in "recommended models" list.
//!
//! This is static data rather than database rows so that a new release can
//! refresh the recommendations without a migration. Only the repository and the
//! preferred quantization are pinned — the exact `.gguf` filename is resolved
//! against the Hugging Face API at pull time, so these entries keep working
//! when a repository renames or re-uploads its files.

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
        context_length: 131072,
        approx_size_bytes: 4900 * MB,
        blurb: "A well-rounded model if you have the memory for it.",
    },
];

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
}
