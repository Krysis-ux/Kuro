//! Kuro LLM core.
//!
//! Everything that is not HTTP routing or CLI argument parsing lives here so
//! that the server and the CLI share one implementation of model management,
//! engine supervision and storage.

pub mod catalog;
pub mod cloud;
pub mod db;
pub mod engine;
pub mod error;
pub mod hardware;
pub mod http;
pub mod mcp;
pub mod paths;
pub mod prompt;
pub mod secrets;
pub mod settings;
pub mod skills;
pub mod sse;
pub mod tools;

pub use db::Db;
pub use error::{KuroError, Result};
pub use paths::Paths;
pub use secrets::SecretStore;
