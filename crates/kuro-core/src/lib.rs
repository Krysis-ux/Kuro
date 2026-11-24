//! Kuro LLM core: everything except HTTP routing and CLI parsing, shared by the
//! server and the CLI.

pub mod agents;
pub mod catalog;
pub mod classify;
pub mod cloud;
pub mod db;
pub mod engine;
pub mod error;
pub mod free;
pub mod gateway;
pub mod hardware;
pub mod http;
pub mod mcp;
pub mod orchestrate;
pub mod paths;
pub mod prompt;
pub mod secrets;
pub mod settings;
pub mod skills;
pub mod sse;
pub mod tools;
pub mod wire;
pub mod workspace;

pub use db::Db;
pub use error::{KuroError, Result};
pub use paths::Paths;
pub use secrets::SecretStore;
