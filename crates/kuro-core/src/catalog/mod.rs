//! Discovering and fetching model weights.

pub mod curated;
pub mod download;
pub mod hf;
pub mod pull;
pub mod search;

pub use curated::{find_curated, CuratedModel, CURATED_MODELS};
pub use download::{download_to_file, sha256_file, DownloadOutcome};
pub use hf::{
    choose_gguf, list_gguf_files, parse_model_ref, quant_from_filename, resolve_download_url,
    GgufFile, ModelRef,
};
pub use pull::{
    execute_pull, plan_pull, prepare_pull, remove_model, run_pull, PreparedPull, PullPlan,
};
