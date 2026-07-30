//! Resolving what to download from a Hugging Face repository.
//!
//! Kuro never hardcodes `.gguf` filenames. The user (or the curated list) names
//! a repository and a preferred quantization, and the exact file is looked up
//! against the repository tree at pull time. That keeps the catalog working
//! when a repository re-uploads or renames its files.

use serde::Deserialize;

use crate::{KuroError, Result};

const HF_HOST: &str = "https://huggingface.co";

/// What the user asked for, before it is resolved to a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelRef {
    /// A slug from the curated list, e.g. `qwen3-4b`.
    Curated { slug: String, quant: Option<String> },
    /// An arbitrary repository, e.g. `unsloth/Qwen3-4B-Instruct-2507-GGUF`.
    HuggingFace { repo: String, quant: Option<String> },
}

/// Parse `qwen3-4b`, `qwen3-4b:Q5_K_M`, `owner/repo`, `owner/repo:Q4_K_M` or a
/// full `huggingface.co` URL.
pub fn parse_model_ref(raw: &str) -> Result<ModelRef> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(KuroError::bad_request("model name is empty"));
    }

    // Accept a pasted URL by reducing it to `owner/repo`.
    let without_host = trimmed
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("huggingface.co/")
        .trim_start_matches("hf.co/")
        .trim_end_matches('/');

    // Strip a trailing `/tree/main` and similar from a copied browser URL.
    let without_host = match without_host.find("/tree/") {
        Some(index) => &without_host[..index],
        None => without_host,
    };
    let without_host = match without_host.find("/blob/") {
        Some(index) => &without_host[..index],
        None => without_host,
    };

    // A quantization suffix is the part after the last colon, and never
    // contains a slash — that distinguishes it from anything URL-shaped.
    let (name, quant) = match without_host.rsplit_once(':') {
        Some((head, tail)) if !tail.contains('/') && !tail.is_empty() && !head.is_empty() => {
            (head, Some(tail.to_string()))
        }
        _ => (without_host, None),
    };

    if name.is_empty() {
        return Err(KuroError::bad_request("model name is empty"));
    }

    if name.contains('/') {
        let segments: Vec<&str> = name.split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() != 2 {
            return Err(KuroError::bad_request(format!(
                "expected a Hugging Face repository like `owner/repo`, got `{name}`"
            )));
        }
        Ok(ModelRef::HuggingFace {
            repo: segments.join("/"),
            quant,
        })
    } else {
        Ok(ModelRef::Curated {
            slug: name.to_string(),
            quant,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TreeEntry {
    #[serde(rename = "type")]
    entry_type: String,
    path: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    lfs: Option<LfsInfo>,
}

#[derive(Debug, Clone, Deserialize)]
struct LfsInfo {
    #[serde(default)]
    size: u64,
    #[serde(default)]
    sha256: Option<String>,
}

/// A downloadable weights file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufFile {
    pub filename: String,
    pub size: u64,
    /// Present for LFS-tracked files, which is how large weights are stored.
    /// When present it is used to verify the download.
    pub sha256: Option<String>,
}

/// List every `.gguf` file in a repository.
pub async fn list_gguf_files(client: &reqwest::Client, repo: &str) -> Result<Vec<GgufFile>> {
    let url = format!("{HF_HOST}/api/models/{repo}/tree/main?recursive=true");
    let response = client.get(&url).send().await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(KuroError::not_found(format!(
            "Hugging Face repository `{repo}`"
        )));
    }
    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(KuroError::model(format!(
            "`{repo}` is gated or private. Accept its licence on Hugging Face, or choose another model."
        )));
    }
    if !response.status().is_success() {
        return Err(KuroError::model(format!(
            "Hugging Face returned {} for `{repo}`",
            response.status()
        )));
    }

    let entries: Vec<TreeEntry> = response.json().await?;
    let files = entries
        .into_iter()
        .filter(|entry| entry.entry_type == "file")
        .filter(|entry| entry.path.to_ascii_lowercase().ends_with(".gguf"))
        .map(|entry| {
            let lfs_size = entry.lfs.as_ref().map(|l| l.size).unwrap_or(0);
            GgufFile {
                size: entry.size.max(lfs_size),
                sha256: entry.lfs.and_then(|l| l.sha256),
                filename: entry.path,
            }
        })
        .collect();

    Ok(files)
}

/// Quantizations to try, best-value first, when the caller has no preference.
const QUANT_PREFERENCE: &[&str] = &["Q4_K_M", "Q4_K_S", "Q5_K_M", "Q6_K", "Q8_0"];

/// Pick the file to download.
pub fn choose_gguf(files: &[GgufFile], preferred_quant: Option<&str>) -> Result<GgufFile> {
    if files.is_empty() {
        return Err(KuroError::model(
            "this repository contains no .gguf files. Kuro can only run GGUF weights.",
        ));
    }

    // Weights above ~50 GB are published as numbered shards. Loading those needs
    // multi-part handling Kuro does not have yet, so say so plainly instead of
    // downloading one shard and producing a model that cannot load.
    let single_part: Vec<&GgufFile> = files.iter().filter(|f| !is_split_part(&f.filename)).collect();
    if single_part.is_empty() {
        return Err(KuroError::model(
            "this model is published as split GGUF shards, which Kuro cannot load yet. \
             Try a smaller quantization that fits in a single file.",
        ));
    }

    if let Some(quant) = preferred_quant {
        if let Some(found) = find_by_quant(&single_part, quant) {
            return Ok(found.clone());
        }
        let available = available_quants(&single_part);
        return Err(KuroError::model(format!(
            "no `{quant}` file in this repository. Available: {}",
            if available.is_empty() {
                "none detected".to_string()
            } else {
                available.join(", ")
            }
        )));
    }

    for quant in QUANT_PREFERENCE {
        if let Some(found) = find_by_quant(&single_part, quant) {
            return Ok(found.clone());
        }
    }

    // Nothing recognisable: take the smallest file, which is the safest default
    // on a machine whose memory we do not want to overrun.
    let smallest = single_part
        .iter()
        .min_by_key(|file| file.size)
        .expect("single_part is non-empty");
    Ok((*smallest).clone())
}

fn find_by_quant<'a>(files: &[&'a GgufFile], quant: &str) -> Option<&'a GgufFile> {
    let needle = quant.to_ascii_lowercase();
    files
        .iter()
        .find(|file| {
            let name = file.filename.to_ascii_lowercase();
            // Match on a delimited token so `Q4_K_M` never matches `Q4_K_M_XL`.
            name.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|token| token == needle)
        })
        .copied()
}

/// Quantization labels detectable from the filenames present.
pub fn available_quants(files: &[&GgufFile]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for file in files {
        if let Some(quant) = quant_from_filename(&file.filename) {
            if !found.iter().any(|q| q.eq_ignore_ascii_case(&quant)) {
                found.push(quant);
            }
        }
    }
    found
}

/// Extract a quantization label such as `Q4_K_M` or `IQ3_XXS` from a filename.
pub fn quant_from_filename(filename: &str) -> Option<String> {
    let stem = filename.rsplit('/').next().unwrap_or(filename);
    let stem = stem.strip_suffix(".gguf").unwrap_or(stem);

    stem.split(['.', '-'])
        .rev()
        .find(|token| {
            let upper = token.to_ascii_uppercase();
            (upper.starts_with('Q') || upper.starts_with("IQ") || upper.starts_with("BF") || upper.starts_with('F'))
                && upper.chars().any(|c| c.is_ascii_digit())
        })
        .map(|token| token.to_ascii_uppercase())
}

/// True for one shard of a multi-part GGUF, e.g. `model-00002-of-00003.gguf`.
fn is_split_part(filename: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    let Some(stem) = lower.strip_suffix(".gguf") else {
        return false;
    };
    // Look for the `-<digits>-of-<digits>` tail that llama.cpp uses.
    let mut parts = stem.rsplit('-');
    let Some(total) = parts.next() else {
        return false;
    };
    let Some(of) = parts.next() else {
        return false;
    };
    let Some(index) = parts.next() else {
        return false;
    };
    of == "of"
        && !total.is_empty()
        && !index.is_empty()
        && total.chars().all(|c| c.is_ascii_digit())
        && index.chars().all(|c| c.is_ascii_digit())
}

pub fn resolve_download_url(repo: &str, filename: &str) -> String {
    format!("{HF_HOST}/{repo}/resolve/main/{filename}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> GgufFile {
        GgufFile {
            filename: name.to_string(),
            size,
            sha256: None,
        }
    }

    #[test]
    fn parses_a_curated_slug_with_and_without_quant() {
        assert_eq!(
            parse_model_ref("qwen3-4b").unwrap(),
            ModelRef::Curated {
                slug: "qwen3-4b".to_string(),
                quant: None
            }
        );
        assert_eq!(
            parse_model_ref("qwen3-4b:Q5_K_M").unwrap(),
            ModelRef::Curated {
                slug: "qwen3-4b".to_string(),
                quant: Some("Q5_K_M".to_string())
            }
        );
    }

    #[test]
    fn parses_repositories_and_pasted_urls() {
        let expected = ModelRef::HuggingFace {
            repo: "unsloth/Qwen3-4B-Instruct-2507-GGUF".to_string(),
            quant: None,
        };
        for input in [
            "unsloth/Qwen3-4B-Instruct-2507-GGUF",
            "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF",
            "huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/",
            "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/tree/main",
        ] {
            assert_eq!(parse_model_ref(input).unwrap(), expected, "input: {input}");
        }
    }

    #[test]
    fn keeps_the_quantization_from_a_repository_reference() {
        assert_eq!(
            parse_model_ref("https://huggingface.co/owner/repo:Q8_0").unwrap(),
            ModelRef::HuggingFace {
                repo: "owner/repo".to_string(),
                quant: Some("Q8_0".to_string())
            }
        );
    }

    #[test]
    fn rejects_empty_and_malformed_references() {
        assert!(parse_model_ref("").is_err());
        assert!(parse_model_ref("   ").is_err());
        assert!(parse_model_ref("a/b/c").is_err());
    }

    #[test]
    fn prefers_the_requested_quantization() {
        let files = vec![
            file("model-Q4_K_M.gguf", 100),
            file("model-Q8_0.gguf", 200),
        ];
        let chosen = choose_gguf(&files, Some("Q8_0")).unwrap();
        assert_eq!(chosen.filename, "model-Q8_0.gguf");
    }

    #[test]
    fn falls_back_to_a_sensible_default_quantization() {
        let files = vec![
            file("model-Q8_0.gguf", 900),
            file("model-Q4_K_M.gguf", 400),
            file("model-F16.gguf", 1600),
        ];
        let chosen = choose_gguf(&files, None).unwrap();
        assert_eq!(chosen.filename, "model-Q4_K_M.gguf", "Q4_K_M is the best default");
    }

    #[test]
    fn picks_the_smallest_when_no_quantization_is_recognisable() {
        let files = vec![file("weights-big.gguf", 900), file("weights-small.gguf", 100)];
        let chosen = choose_gguf(&files, None).unwrap();
        assert_eq!(chosen.filename, "weights-small.gguf");
    }

    #[test]
    fn reports_available_quantizations_when_the_request_cannot_be_met() {
        let files = vec![file("model-Q4_K_M.gguf", 100)];
        let error = choose_gguf(&files, Some("Q2_K")).unwrap_err().to_string();
        assert!(error.contains("Q4_K_M"), "error should list what is available: {error}");
    }

    #[test]
    fn refuses_split_shards_rather_than_downloading_a_broken_model() {
        let files = vec![
            file("model-Q4_K_M-00001-of-00003.gguf", 100),
            file("model-Q4_K_M-00002-of-00003.gguf", 100),
        ];
        let error = choose_gguf(&files, None).unwrap_err().to_string();
        assert!(error.contains("split"), "got: {error}");
    }

    #[test]
    fn ignores_shards_but_still_uses_single_files_in_the_same_repo() {
        let files = vec![
            file("model-Q8_0-00001-of-00002.gguf", 500),
            file("model-Q4_K_M.gguf", 100),
        ];
        let chosen = choose_gguf(&files, None).unwrap();
        assert_eq!(chosen.filename, "model-Q4_K_M.gguf");
    }

    #[test]
    fn detects_split_parts_precisely() {
        assert!(is_split_part("model-00001-of-00002.gguf"));
        assert!(!is_split_part("model-Q4_K_M.gguf"));
        assert!(!is_split_part("llama-3-of-legends.gguf"));
    }

    #[test]
    fn extracts_quantization_labels_from_filenames() {
        assert_eq!(
            quant_from_filename("Qwen3-4B-Instruct-2507-Q4_K_M.gguf").as_deref(),
            Some("Q4_K_M")
        );
        assert_eq!(
            quant_from_filename("some/dir/model.IQ3_XXS.gguf").as_deref(),
            Some("IQ3_XXS")
        );
        assert_eq!(quant_from_filename("plain.gguf"), None);
    }

    #[test]
    fn empty_repository_is_an_understandable_error() {
        let error = choose_gguf(&[], None).unwrap_err().to_string();
        assert!(error.contains("GGUF"));
    }

    #[test]
    fn builds_a_resolve_url() {
        assert_eq!(
            resolve_download_url("owner/repo", "model.gguf"),
            "https://huggingface.co/owner/repo/resolve/main/model.gguf"
        );
    }
}
