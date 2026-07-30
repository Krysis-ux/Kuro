//! Thin HTTP client for the local Kuro daemon.
//!
//! The CLI holds no model or engine logic of its own — it is another consumer
//! of the same API the web interface uses, so the two can never drift apart.

use anyhow::{bail, Context, Result};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8420";

pub struct KuroClient {
    base_url: String,
    http: reqwest::Client,
}

impl KuroClient {
    pub fn new() -> Result<Self> {
        let base_url = std::env::var("KURO_HOST").unwrap_or_else(|_| {
            match std::env::var("KURO_PORT").ok().and_then(|p| p.parse::<u16>().ok()) {
                Some(port) => format!("http://127.0.0.1:{port}"),
                None => DEFAULT_BASE_URL.to_string(),
            }
        });

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            // No global timeout: generation and downloads both run long.
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .no_proxy()
                .build()?,
        })
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Fail with instructions rather than a connection error when the daemon is
    /// not running, which is the most common first-run mistake.
    pub async fn ensure_running(&self) -> Result<()> {
        match self.http.get(self.url("/api/health")).send().await {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => bail!(
                "Kuro is reachable at {} but returned {}",
                self.base_url,
                response.status()
            ),
            Err(_) => bail!(
                "Kuro is not running at {}.\n\nStart it with:\n    kuro serve",
                self.base_url
            ),
        }
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        let response = self
            .http
            .get(self.url(path))
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        decode(response).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let response = self
            .http
            .post(self.url(path))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {path}"))?;
        decode(response).await
    }

    pub async fn delete(&self, path: &str) -> Result<Value> {
        let response = self
            .http
            .delete(self.url(path))
            .send()
            .await
            .with_context(|| format!("DELETE {path}"))?;
        decode(response).await
    }
}

/// Turn a response into JSON, surfacing the server's own error message.
async fn decode(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if status.is_success() {
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        return serde_json::from_str(&body).context("the server sent a malformed response");
    }

    // Errors follow OpenAI's shape; fall back to the raw body if that changes.
    let message = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().to_string());

    if message.is_empty() {
        bail!("request failed with status {status}");
    }
    bail!("{message}");
}

/// Human-readable byte size.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [(&str, u64); 4] = [
        ("TB", 1024 * 1024 * 1024 * 1024),
        ("GB", 1024 * 1024 * 1024),
        ("MB", 1024 * 1024),
        ("KB", 1024),
    ];

    for (label, size) in UNITS {
        if bytes >= size {
            return format!("{:.1} {label}", bytes as f64 / size as f64);
        }
    }
    format!("{bytes} B")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes_at_each_scale() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
