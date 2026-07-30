use thiserror::Error;

/// Every fallible operation in Kuro returns this error.
///
/// Variants are grouped by how a caller should react, not by which library
/// produced them: `NotFound` and `BadRequest` map onto 404/400 at the HTTP
/// layer, everything else is a 500.
#[derive(Debug, Error)]
pub enum KuroError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid request: {0}")]
    BadRequest(String),

    /// The inference engine could not be downloaded, started or reached.
    #[error("engine error: {0}")]
    Engine(String),

    /// A model file could not be resolved, downloaded or verified.
    #[error("model error: {0}")]
    Model(String),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, KuroError>;

impl KuroError {
    pub fn not_found(what: impl std::fmt::Display) -> Self {
        Self::NotFound(what.to_string())
    }

    pub fn bad_request(why: impl std::fmt::Display) -> Self {
        Self::BadRequest(why.to_string())
    }

    pub fn engine(why: impl std::fmt::Display) -> Self {
        Self::Engine(why.to_string())
    }

    pub fn model(why: impl std::fmt::Display) -> Self {
        Self::Model(why.to_string())
    }

    pub fn other(why: impl std::fmt::Display) -> Self {
        Self::Other(why.to_string())
    }
}
