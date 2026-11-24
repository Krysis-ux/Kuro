//! Shared outbound HTTP clients. GitHub rejects requests without a
//! `User-Agent`, so use these instead of `reqwest::Client::new()`.

use std::time::Duration;

use crate::Result;

pub const USER_AGENT: &str = concat!("kuro-llm/", env!("CARGO_PKG_VERSION"));

/// Client for Hugging Face, GitHub and cloud provider APIs. No overall request
/// timeout: it also streams multi-gigabyte downloads.
pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .pool_idle_timeout(Duration::from_secs(60))
        .build()
        .map_err(Into::into)
}

/// Client for a local engine process. Short connect timeout, no read timeout
/// (generation can take minutes).
pub fn loopback_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(5))
        .no_proxy()
        .build()
        .map_err(Into::into)
}
