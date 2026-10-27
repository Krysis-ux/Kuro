//! Everything Hugging Face publishes about one repository — the model card,
//! the papers it cites, what it was quantized from and every weights file — so
//! the detail screen can answer "what is this" without sending anyone to the Hub.

use futures::future::join_all;
use serde::{Deserialize, Serialize};

use super::hf::{self, GgufFile};
use crate::{KuroError, Result};

const HF_HOST: &str = "https://huggingface.co";
/// Cards often cite related work after their own paper; the first few are the ones that matter.
const MAX_PAPERS: usize = 4;
const MAX_SPACES: usize = 8;

#[derive(Debug, Deserialize)]
struct RepoInfo {
    id: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    likes: u64,
    #[serde(default, rename = "createdAt")]
    created_at: Option<String>,
    #[serde(default, rename = "lastModified")]
    last_modified: Option<String>,
    #[serde(default)]
    pipeline_tag: Option<String>,
    #[serde(default)]
    library_name: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    gated: Option<serde_json::Value>,
    #[serde(default, rename = "cardData")]
    card_data: Option<CardData>,
    #[serde(default)]
    gguf: Option<GgufMeta>,
    #[serde(default)]
    spaces: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CardData {
    #[serde(default)]
    license: Option<OneOrMany>,
    #[serde(default)]
    base_model: Option<OneOrMany>,
    #[serde(default)]
    datasets: Option<OneOrMany>,
    #[serde(default)]
    language: Option<OneOrMany>,
}

/// Card metadata fields are written by hand and come as either a string or a list.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn into_vec(self) -> Vec<String> {
        match self {
            OneOrMany::One(value) => vec![value],
            OneOrMany::Many(values) => values,
        }
    }
}

/// What the Hub reads out of a GGUF header.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GgufMeta {
    /// Parameter count.
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub architecture: Option<String>,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub chat_template: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PaperInfo {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default, rename = "publishedAt")]
    published_at: Option<String>,
    #[serde(default)]
    upvotes: Option<u64>,
    #[serde(default)]
    authors: Vec<PaperAuthor>,
}

#[derive(Debug, Deserialize)]
struct PaperAuthor {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Paper {
    /// The arXiv id, e.g. `2501.12948`.
    pub id: String,
    /// Missing when the Hub has no page for the paper; the arXiv link still works.
    pub title: Option<String>,
    pub summary: Option<String>,
    pub published_at: Option<String>,
    pub upvotes: Option<u64>,
    pub authors: Vec<String>,
}

/// One downloadable weights file.
#[derive(Debug, Clone, Serialize)]
pub struct CardFile {
    pub filename: String,
    pub quant: Option<String>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelCard {
    pub repo: String,
    pub author: Option<String>,
    pub downloads: u64,
    pub likes: u64,
    pub created_at: Option<String>,
    pub last_modified: Option<String>,
    pub pipeline_tag: Option<String>,
    pub library_name: Option<String>,
    pub license: Option<String>,
    /// Plain topic tags; the `key:value` ones are pulled out into their own fields.
    pub tags: Vec<String>,
    pub gated: bool,
    pub base_models: Vec<String>,
    pub datasets: Vec<String>,
    pub languages: Vec<String>,
    pub spaces: Vec<String>,
    pub gguf: Option<GgufMeta>,
    /// The model card as markdown, front matter removed.
    pub readme: Option<String>,
    pub papers: Vec<Paper>,
    /// Single-file GGUF weights, smallest first. Vision projectors are not listed.
    pub files: Vec<CardFile>,
    /// Quantizations published only as split shards, which cannot be loaded yet.
    pub split_quants: Vec<String>,
    /// Whether a vision projector (`mmproj`) ships alongside the weights.
    pub has_projector: bool,
}

/// Gather the card for `repo` (`owner/name`, or a pasted Hub URL).
pub async fn fetch_model_card(client: &reqwest::Client, raw: &str) -> Result<ModelCard> {
    let repo = validate_repo(raw)?;

    let (info, readme, files) = tokio::join!(
        fetch_info(client, &repo),
        fetch_readme(client, &repo),
        hf::list_gguf_files(client, &repo),
    );
    let info = info?;
    // A card without its file list is still worth showing; gated repos refuse the listing.
    let files = files.unwrap_or_else(|error| {
        tracing::debug!(%repo, %error, "no file list for model card");
        Vec::new()
    });

    let card = info.card_data.unwrap_or_default();
    let base_models = card.base_model.map(OneOrMany::into_vec).unwrap_or_default();

    // A quantized repo rarely carries the paper itself; its base model usually does.
    let mut paper_ids = arxiv_ids(&info.tags);
    if paper_ids.is_empty() {
        // Validated like user input: the card's metadata is whatever its author typed.
        if let Some(base) = base_models.first().and_then(|base| validate_repo(base).ok()) {
            if let Ok(base_info) = fetch_info(client, &base).await {
                paper_ids = arxiv_ids(&base_info.tags);
            }
        }
    }
    paper_ids.truncate(MAX_PAPERS);
    let papers = join_all(paper_ids.into_iter().map(|id| fetch_paper(client, id))).await;

    let license = card
        .license
        .and_then(|value| value.into_vec().into_iter().next())
        .or_else(|| tag_value(&info.tags, "license"));
    let (files, split_quants, has_projector) = sort_files(files);

    Ok(ModelCard {
        author: info.author,
        downloads: info.downloads,
        likes: info.likes,
        created_at: info.created_at,
        last_modified: info.last_modified,
        pipeline_tag: info.pipeline_tag,
        library_name: info.library_name,
        license,
        tags: info.tags.iter().filter(|tag| !tag.contains(':')).cloned().collect(),
        gated: matches!(&info.gated, Some(serde_json::Value::Bool(true)))
            || matches!(&info.gated, Some(serde_json::Value::String(mode)) if mode != "false"),
        base_models,
        datasets: card.datasets.map(OneOrMany::into_vec).unwrap_or_default(),
        languages: card.language.map(OneOrMany::into_vec).unwrap_or_default(),
        spaces: info.spaces.into_iter().take(MAX_SPACES).collect(),
        gguf: info.gguf,
        readme: readme.map(|text| strip_front_matter(&text).to_string()),
        papers,
        files,
        split_quants,
        has_projector,
        repo: info.id,
    })
}

/// Reduce to `owner/name` and refuse anything that could steer the request
/// elsewhere on the Hub (`..`, extra segments).
fn validate_repo(raw: &str) -> Result<String> {
    let hf::ModelRef::HuggingFace { repo, .. } = hf::parse_model_ref(raw)? else {
        return Err(KuroError::bad_request("expected a repository like `owner/name`"));
    };
    let valid = repo.split('/').all(|segment| {
        !segment.is_empty()
            && !segment.starts_with('.')
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    });
    if !valid {
        return Err(KuroError::bad_request(format!("`{repo}` is not a repository name")));
    }
    Ok(repo)
}

async fn fetch_info(client: &reqwest::Client, repo: &str) -> Result<RepoInfo> {
    let response = client.get(format!("{HF_HOST}/api/models/{repo}")).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND
        || response.status() == reqwest::StatusCode::UNAUTHORIZED
    {
        return Err(KuroError::not_found(format!("Hugging Face repository `{repo}`")));
    }
    if !response.status().is_success() {
        return Err(KuroError::other(format!(
            "Hugging Face returned {} for `{repo}`",
            response.status()
        )));
    }
    Ok(response.json().await?)
}

/// The card is optional on the Hub, so a missing one is `None` rather than an error.
async fn fetch_readme(client: &reqwest::Client, repo: &str) -> Option<String> {
    let response = client
        .get(format!("{HF_HOST}/{repo}/raw/main/README.md"))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.text().await.ok()
}

/// A paper the Hub has no page for still gets its id, which is enough for an arXiv link.
async fn fetch_paper(client: &reqwest::Client, id: String) -> Paper {
    let info: Option<PaperInfo> = async {
        let response = client.get(format!("{HF_HOST}/api/papers/{id}")).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json().await.ok()
    }
    .await;

    match info {
        Some(info) => Paper {
            title: info.title,
            summary: info.summary,
            published_at: info.published_at,
            upvotes: info.upvotes,
            authors: info.authors.into_iter().map(|author| author.name).collect(),
            id,
        },
        None => Paper {
            id,
            title: None,
            summary: None,
            published_at: None,
            upvotes: None,
            authors: Vec::new(),
        },
    }
}

fn arxiv_ids(tags: &[String]) -> Vec<String> {
    tags.iter()
        .filter_map(|tag| tag.strip_prefix("arxiv:"))
        .map(str::to_string)
        .collect()
}

fn tag_value(tags: &[String], key: &str) -> Option<String> {
    tags.iter()
        .find_map(|tag| tag.strip_prefix(key)?.strip_prefix(':'))
        .map(str::to_string)
}

/// Split the listing into loadable weights, split-only quantizations and projectors.
fn sort_files(files: Vec<GgufFile>) -> (Vec<CardFile>, Vec<String>, bool) {
    let mut single = Vec::new();
    let mut split_quants: Vec<String> = Vec::new();
    let mut has_projector = false;

    for file in files {
        let basename = file.filename.rsplit('/').next().unwrap_or(&file.filename);
        if basename.to_ascii_lowercase().starts_with("mmproj") {
            has_projector = true;
            continue;
        }
        let quant = hf::quant_from_filename(&file.filename);
        if hf::is_split_part(&file.filename) {
            if let Some(quant) = quant {
                if !split_quants.contains(&quant) {
                    split_quants.push(quant);
                }
            }
            continue;
        }
        single.push(CardFile {
            filename: file.filename,
            quant,
            size: file.size,
        });
    }

    single.sort_by_key(|file| file.size);
    // A quantization offered as one file is not "split only", whatever else is in the repo.
    split_quants.retain(|quant| !single.iter().any(|file| file.quant.as_ref() == Some(quant)));
    (single, split_quants, has_projector)
}

fn strip_front_matter(readme: &str) -> &str {
    let Some(rest) = readme.strip_prefix("---") else {
        return readme;
    };
    match rest.find("\n---") {
        Some(end) => rest[end + 4..].trim_start_matches(['\r', '\n']),
        None => readme,
    }
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
    fn strips_yaml_front_matter() {
        assert_eq!(strip_front_matter("---\nlicense: mit\n---\n\n# Title"), "# Title");
        assert_eq!(strip_front_matter("# No front matter"), "# No front matter");
        assert_eq!(strip_front_matter("---\nunterminated"), "---\nunterminated");
    }

    #[test]
    fn rejects_repositories_that_could_escape_the_api_path() {
        assert_eq!(validate_repo("unsloth/Qwen3-4B-GGUF").unwrap(), "unsloth/Qwen3-4B-GGUF");
        assert_eq!(
            validate_repo("https://huggingface.co/unsloth/Qwen3-4B-GGUF/tree/main").unwrap(),
            "unsloth/Qwen3-4B-GGUF"
        );
        assert!(validate_repo("owner/..").is_err());
        assert!(validate_repo("owner/name?x=1").is_err());
        assert!(validate_repo("qwen3-4b").is_err());
    }

    #[test]
    fn sorts_weights_and_sets_aside_projectors_and_shards() {
        let (files, split, projector) = sort_files(vec![
            file("Model-Q8_0.gguf", 800),
            file("Model-Q4_K_M.gguf", 400),
            file("mmproj-F16.gguf", 50),
            file("BF16/Model-BF16-00001-of-00002.gguf", 900),
            file("BF16/Model-BF16-00002-of-00002.gguf", 900),
        ]);
        let names: Vec<_> = files.iter().map(|f| f.quant.as_deref().unwrap()).collect();
        assert_eq!(names, ["Q4_K_M", "Q8_0"]);
        assert_eq!(split, ["BF16"]);
        assert!(projector);
    }

    #[test]
    fn reads_papers_and_licence_from_tags() {
        let tags = ["arxiv:2501.12948", "license:mit", "gguf"].map(String::from);
        assert_eq!(arxiv_ids(&tags), ["2501.12948"]);
        assert_eq!(tag_value(&tags, "license").as_deref(), Some("mit"));
    }
}
