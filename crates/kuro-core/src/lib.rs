//! Kuro LLM core.
//!
//! Everything that is not HTTP routing or CLI argument parsing lives here so
//! that the server and the CLI share one implementation of model management,
//! engine supervision and storage.

pub mod catalog;
pub mod db;
pub mod engine;
pub mod error;
pub mod hardware;
pub mod http;
pub mod paths;
pub mod settings;
pub mod sse;

pub use db::Db;
pub use error::{KuroError, Result};
pub use paths::Paths;
