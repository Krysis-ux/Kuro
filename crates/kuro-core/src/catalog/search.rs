//! Finding GGUF models on Hugging Face.
//!
//! The Models page previously offered one input that accepted a repository path,
//! which is only useful if you already know what you want. This searches the Hub
//! from inside Kuro instead, filtered to GGUF so nothing in the results is a model
//! that cannot run here.
//!
//! Results carry a size and a fit verdict, because "will this run on my machine"
//! is the only question that matters at the point of choosing, and the Hub cannot
//! answer it.

use serde::{Deserialize, Serialize};

use crate::hardware::{fit, HardwareInfo};
use crate::{KuroError, Result};

const HF_API: &str = "https://huggingface.co/api/models";
/// The Hub tag that marks a repository as containing GGUF weights.
const GGUF_FILTER: &str = "gguf";
const DEFAULT_LIMIT: usize = 24;
const MAX_LIMIT: usize = 50;

/// One repository from the Hub.
#[derive(Debug, Clone, Deserialize)]
struct HubModel {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    likes: u64,
    #[serde(default, rename = "lastModified")]
    last_modified: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    gated: Option<serde_json::Value>,
    #[serde(default)]
    siblings: Vec<HubSibling>,
}

#[derive(Debug, Clone, Deserialize)]
struct HubSibling {
    #[serde(default, rename = "rfilename")]
    filename: String,
}

/// A searchable, installable model.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    /// `owner/repo`, which is exactly what the pull endpoint accepts.
    pub repo: String,
    /// The repository name without its owner, for display.
    pub name: String,
    pub owner: String,
    pub downloads: u64,
    pub likes: u64,
    pub last_modified: Option<String>,
    /// Quantizations detected in the repository's file list.
    pub quants: Vec<String>,
    /// Params inferred from the name, when the name says so.
    pub param_count: Option<String>,
    /// True when the repository needs a licence accepted before it can be
    /// downloaded, which Kuro cannot do on the user's behalf.
    pub gated: bool,
    /// Set when the repository publishes only multi-part weights, which cannot be
    /// loaded yet. Shown rather than hidden so the absence is explained.
    pub split_only: bool,
}

/// Search the Hub for GGUF repositories.
pub async fn search_gguf(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Err(KuroError::bad_request("the search query is empty"));
    }

    let limit = limit.clamp(1, MAX_LIMIT);
    let response = client
        .get(HF_API)
        .query(&[
            ("search", query),
            ("filter", GGUF_FILTER),
            ("limit", &limit.to_string()),
            ("sort", "downloads"),
            ("direction", "-1"),
            // The file list is what makes a result useful — it is where the
            // quantizations come from — so it is requested up front rather than
            // with a follow-up call per repository.
            ("full", "true"),
        ])
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        return Err(KuroError::other(format!(
            "Hugging Face returned {status} for that search"
        )));
    }

    let models: Vec<HubModel> = serde_json::from_str(&body)
        .map_err(|error| KuroError::other(format!("could not read the search results: {error}")))?;

    Ok(models.into_iter().map(to_hit).collect())
}

/// The most-downloaded GGUF repositories, for a browse view with no query.
pub async fn trending_gguf(client: &reqwest::Client, limit: usize) -> Result<Vec<SearchHit>> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let response = client
        .get(HF_API)
        .query(&[
            ("filter", GGUF_FILTER),
            ("limit", &limit.to_string()),
            ("sort", "downloads"),
            ("direction", "-1"),
            ("full", "true"),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(KuroError::other(format!(
            "Hugging Face returned {}",
            response.status()
        )));
    }

    let models: Vec<HubModel> = response.json().await?;
    Ok(models.into_iter().map(to_hit).collect())
}

pub fn default_limit() -> usize {
    DEFAULT_LIMIT
}

fn to_hit(model: HubModel) -> SearchHit {
    let files: Vec<&str> = model
        .siblings
        .iter()
        .map(|sibling| sibling.filename.as_str())
        .filter(|name| name.to_ascii_lowercase().ends_with(".gguf"))
        .collect();

    let mut quants: Vec<String> = Vec::new();
    let mut has_single_part = false;

    for file in &files {
        if is_split_part(file) {
            continue;
        }
        has_single_part = true;
        if let Some(quant) = super::hf::quant_from_filename(file) {
            if !quants.iter().any(|held| held.eq_ignore_ascii_case(&quant)) {
                quants.push(quant);
            }
        }
    }

    quants.sort_by_key(|quant| quant_rank(quant));

    let (owner, name) = match model.id.split_once('/') {
        Some((owner, name)) => (owner.to_string(), name.to_string()),
        None => (String::new(), model.id.clone()),
    };

    SearchHit {
        param_count: param_count_from_name(&name),
        gated: is_gated(&model.gated) || model.tags.iter().any(|tag| tag == "gated"),
        split_only: !files.is_empty() && !has_single_part,
        repo: model.id,
        name,
        owner,
        downloads: model.downloads,
        likes: model.likes,
        last_modified: model.last_modified,
        quants,
    }
}

/// The Hub reports `gated` as `false`, `"auto"` or `"manual"`.
fn is_gated(raw: &Option<serde_json::Value>) -> bool {
    match raw {
        Some(serde_json::Value::Bool(flag)) => *flag,
        Some(serde_json::Value::String(mode)) => mode != "false",
        _ => false,
    }
}

/// Order quantizations from smallest to largest, so a list reads as a size ramp.
fn quant_rank(quant: &str) -> u8 {
    let upper = quant.to_ascii_uppercase();
    // The leading digit is the bit width, which is what determines size.
    let width = upper
        .chars()
        .find(|c| c.is_ascii_digit())
        .and_then(|c| c.to_digit(10))
        .unwrap_or(9) as u8;

    // F16 and BF16 are unquantized and belong at the end regardless.
    if upper.starts_with('F') || upper.starts_with("BF") {
        return 200 + width;
    }
    width * 10
}

/// Parameter count as stated by the repository name, e.g. `Qwen3-4B` → `4B`.
fn param_count_from_name(name: &str) -> Option<String> {
    name.split(['-', '_', '.'])
        .find(|token| {
            let upper = token.to_ascii_uppercase();
            (upper.ends_with('B') || upper.ends_with("B0"))
                && upper.len() <= 6
                && upper.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
        .map(|token| token.to_ascii_uppercase())
}

/// Mirrors `hf::is_split_part`, which is private to that module.
fn is_split_part(filename: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    let Some(stem) = lower.strip_suffix(".gguf") else {
        return false;
    };

    let mut parts = stem.rsplit('-');
    let Some(total) = parts.next() else { return false };
    let Some(of) = parts.next() else { return false };
    let Some(index) = parts.next() else { return false };

    of == "of"
        && !total.is_empty()
        && !index.is_empty()
        && total.chars().all(|c| c.is_ascii_digit())
        && index.chars().all(|c| c.is_ascii_digit())
}

/// Estimate whether a hit will run here, using the size of its smallest
/// single-file quantization.
///
/// Search results carry no file sizes without a request per repository, so the
/// estimate is derived from the parameter count and quantization instead. It is
/// approximate, and the UI says so.
pub fn estimate_fit(hit: &SearchHit, hardware: &HardwareInfo) -> Option<fit::FitEstimate> {
    let params = parse_billions(hit.param_count.as_deref()?)?;
    let quant = hit.quants.first().map(String::as_str).unwrap_or("Q4_K_M");
    let bits = bits_per_weight(quant);

    let bytes = (params * 1_000_000_000.0 * bits / 8.0) as u64;
    Some(fit::estimate_fit(bytes, hardware))
}

fn parse_billions(param_count: &str) -> Option<f64> {
    let digits = param_count.trim_end_matches(['B', 'b']);
    digits.parse::<f64>().ok().filter(|value| *value > 0.0)
}

/// Roughly how many bits each weight takes at a given quantization.
fn bits_per_weight(quant: &str) -> f64 {
    let upper = quant.to_ascii_uppercase();
    if upper.starts_with("BF") || upper.starts_with('F') {
        return 16.0;
    }
    match upper.chars().find(|c| c.is_ascii_digit()).and_then(|c| c.to_digit(10)) {
        // The K-quant families carry a little more than their nominal width.
        Some(2) => 3.0,
        Some(3) => 3.9,
        Some(4) => 4.8,
        Some(5) => 5.7,
        Some(6) => 6.6,
        Some(8) => 8.5,
        _ => 4.8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model(id: &str, files: &[&str]) -> HubModel {
        serde_json::from_value(json!({
            "id": id,
            "downloads": 1000,
            "likes": 10,
            "siblings": files.iter().map(|f| json!({ "rfilename": f })).collect::<Vec<_>>(),
        }))
        .expect("parse")
    }

    #[test]
    fn extracts_quantizations_from_the_file_list() {
        let hit = to_hit(model(
            "unsloth/Qwen3-4B-GGUF",
            &["Qwen3-4B-Q4_K_M.gguf", "Qwen3-4B-Q8_0.gguf", "README.md"],
        ));

        assert_eq!(hit.repo, "unsloth/Qwen3-4B-GGUF");
        assert_eq!(hit.owner, "unsloth");
        assert_eq!(hit.name, "Qwen3-4B-GGUF");
        assert_eq!(hit.quants, vec!["Q4_K_M".to_string(), "Q8_0".to_string()]);
        assert!(!hit.split_only);
    }

    #[test]
    fn quantizations_are_ordered_smallest_first_with_full_precision_last() {
        let hit = to_hit(model(
            "o/r",
            &["m-Q8_0.gguf", "m-F16.gguf", "m-Q2_K.gguf", "m-Q4_K_M.gguf"],
        ));

        assert_eq!(
            hit.quants,
            vec![
                "Q2_K".to_string(),
                "Q4_K_M".to_string(),
                "Q8_0".to_string(),
                "F16".to_string()
            ]
        );
    }

    #[test]
    fn a_repository_of_only_shards_is_flagged_rather_than_hidden() {
        let hit = to_hit(model(
            "o/big",
            &["m-00001-of-00003.gguf", "m-00002-of-00003.gguf"],
        ));

        assert!(hit.split_only, "the user should be told why this one cannot be used");
        assert!(hit.quants.is_empty());
    }

    #[test]
    fn shards_alongside_a_single_file_do_not_flag_the_repository() {
        let hit = to_hit(model(
            "o/mixed",
            &["m-Q8_0-00001-of-00002.gguf", "m-Q4_K_M.gguf"],
        ));

        assert!(!hit.split_only);
        assert_eq!(hit.quants, vec!["Q4_K_M".to_string()]);
    }

    #[test]
    fn a_repository_with_no_gguf_files_is_not_marked_split_only() {
        let hit = to_hit(model("o/plain", &["config.json", "model.safetensors"]));
        assert!(!hit.split_only);
        assert!(hit.quants.is_empty());
    }

    #[test]
    fn reads_the_hubs_several_spellings_of_gated() {
        assert!(!is_gated(&None));
        assert!(!is_gated(&Some(json!(false))));
        assert!(!is_gated(&Some(json!("false"))));
        assert!(is_gated(&Some(json!(true))));
        assert!(is_gated(&Some(json!("auto"))));
        assert!(is_gated(&Some(json!("manual"))));
    }

    #[test]
    fn infers_a_parameter_count_from_the_repository_name() {
        assert_eq!(param_count_from_name("Qwen3-4B-Instruct-GGUF").as_deref(), Some("4B"));
        assert_eq!(param_count_from_name("Llama-3.2-1B-GGUF").as_deref(), Some("1B"));
        assert_eq!(param_count_from_name("bge-small-en-v1.5-GGUF"), None);
    }

    #[test]
    fn a_repository_without_an_owner_still_produces_a_hit() {
        let hit = to_hit(model("standalone", &["m-Q4_K_M.gguf"]));
        assert_eq!(hit.owner, "");
        assert_eq!(hit.name, "standalone");
    }

    #[test]
    fn quantization_width_drives_the_size_estimate() {
        assert!(bits_per_weight("Q2_K") < bits_per_weight("Q4_K_M"));
        assert!(bits_per_weight("Q4_K_M") < bits_per_weight("Q8_0"));
        assert_eq!(bits_per_weight("F16"), 16.0);
        assert_eq!(bits_per_weight("BF16"), 16.0);
        assert_eq!(
            bits_per_weight("unrecognisable"),
            bits_per_weight("Q4_K_M"),
            "an unknown label should assume the common default, not zero"
        );
    }

    #[test]
    fn estimates_fit_from_parameters_and_quantization() {
        let hardware = crate::hardware::detect();

        let small = SearchHit {
            repo: "o/r".to_string(),
            name: "Qwen3-1B".to_string(),
            owner: "o".to_string(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            quants: vec!["Q4_K_M".to_string()],
            param_count: Some("1B".to_string()),
            gated: false,
            split_only: false,
        };
        let huge = SearchHit {
            name: "Behemoth-405B".to_string(),
            param_count: Some("405B".to_string()),
            ..small.clone()
        };

        let small_fit = estimate_fit(&small, &hardware).expect("estimate");
        let huge_fit = estimate_fit(&huge, &hardware).expect("estimate");

        assert!(small_fit.estimated_required_bytes < huge_fit.estimated_required_bytes);
        assert_eq!(huge_fit.verdict, fit::FitVerdict::WontFit);
    }

    #[test]
    fn a_hit_with_no_parameter_count_has_no_estimate_rather_than_a_wrong_one() {
        let hardware = crate::hardware::detect();
        let hit = SearchHit {
            repo: "o/r".to_string(),
            name: "mystery-model".to_string(),
            owner: "o".to_string(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            quants: vec![],
            param_count: None,
            gated: false,
            split_only: false,
        };

        assert!(estimate_fit(&hit, &hardware).is_none());
    }

    #[tokio::test]
    async fn an_empty_query_is_rejected_without_a_network_call() {
        let client = reqwest::Client::new();
        let error = search_gguf(&client, "  ", 10).await.unwrap_err().to_string();
        assert!(error.contains("empty"));
    }
}
