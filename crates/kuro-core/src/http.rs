//! Shared outbound HTTP client.
//!
//! GitHub's API rejects requests without a `User-Agent`, so every outbound call
//! goes through this builder rather than `reqwest::Client::new()`.

use std::time::Duration;

use crate::Result;

pub const USER_AGENT: &str = concat!("kuro-llm/", env!("CARGO_PKG_VERSION"));

/// Client for talking to Hugging Face, GitHub and cloud provider APIs.
///
/// No overall request timeout is set because the same client streams
/// multi-gigabyte downloads; the connect timeout still bounds a dead host.
pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .pool_idle_timeout(Duration::from_secs(60))
        .build()
        .map_err(Into::into)
}

/// Client for talking to a local engine process.
///
/// Loopback only, so connect timeouts are short, but no read timeout: token
/// generation legitimately takes minutes.
pub fn loopback_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(5))
        .no_proxy()
        .build()
        .map_err(Into::into)
}
