//! `kuro pull` — download a model.

use std::io::Write;
use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::client::{format_bytes, KuroClient};

/// How often to refresh the progress line.
const POLL_INTERVAL: Duration = Duration::from_millis(400);

pub async fn pull(client: &KuroClient, reference: &str) -> Result<()> {
    client.ensure_running().await?;

    let started = client
        .post("/api/models/pull", json!({ "model": reference }))
        .await?;

    let download_id = started["downloadId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("the server did not return a download id"))?
        .to_string();

    let model = &started["model"];
    let name = model["id"].as_str().unwrap_or(reference);
    let total = model["sizeBytes"].as_u64().unwrap_or(0);

    println!("Pulling {name}");
    println!(
        "  {} · {} · {}",
        model["repo"].as_str().unwrap_or("-"),
        model["quant"].as_str().unwrap_or("-"),
        format_bytes(total)
    );

    if let Some(fit) = started["fit"]["label"].as_str() {
        println!("  fit: {fit}");
    }

    follow_progress(client, &download_id).await
}

/// Print a live progress line until the transfer finishes.
async fn follow_progress(client: &KuroClient, download_id: &str) -> Result<()> {
    let path = format!("/api/downloads/{download_id}");

    loop {
        let record = client.get(&path).await?;
        let status = record["status"].as_str().unwrap_or("queued");
        let downloaded = record["downloaded_bytes"].as_u64().unwrap_or(0);
        let total = record["total_bytes"].as_u64();

        match status {
            "completed" => {
                clear_line();
                println!("Done. {} downloaded.", format_bytes(downloaded));
                return Ok(());
            }
            "failed" => {
                clear_line();
                bail!(
                    "{}",
                    record["error"].as_str().unwrap_or("the download failed")
                );
            }
            "cancelled" => {
                clear_line();
                println!("Cancelled.");
                return Ok(());
            }
            _ => {
                print_progress(status, downloaded, total);
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }
    }
}

fn print_progress(status: &str, downloaded: u64, total: Option<u64>) {
    let rendered = match total {
        Some(total) if total > 0 => {
            let fraction = (downloaded as f64 / total as f64).clamp(0.0, 1.0);
            format!(
                "  [{}] {:>3.0}%  {} / {}",
                bar(fraction),
                fraction * 100.0,
                format_bytes(downloaded),
                format_bytes(total)
            )
        }
        // Some sources do not send a length; show what has arrived so far.
        _ => format!("  {} ({status})", format_bytes(downloaded)),
    };

    print!("\r\x1b[2K{rendered}");
    let _ = std::io::stdout().flush();
}

fn bar(fraction: f64) -> String {
    const WIDTH: usize = 28;
    let filled = (fraction * WIDTH as f64).round() as usize;
    format!(
        "{}{}",
        "━".repeat(filled.min(WIDTH)),
        "─".repeat(WIDTH.saturating_sub(filled))
    )
}

fn clear_line() {
    print!("\r\x1b[2K");
    let _ = std::io::stdout().flush();
}

/// `kuro preview` — resolve a reference without downloading.
pub async fn preview(client: &KuroClient, reference: &str) -> Result<()> {
    client.ensure_running().await?;
    let plan: Value = client
        .post("/api/models/preview", json!({ "model": reference }))
        .await?;

    println!("{}", plan["id"].as_str().unwrap_or(reference));
    println!("  repository  {}", plan["repo"].as_str().unwrap_or("-"));
    println!("  file        {}", plan["file"].as_str().unwrap_or("-"));
    println!("  quantization {}", plan["quant"].as_str().unwrap_or("-"));
    println!(
        "  size        {}",
        format_bytes(plan["sizeBytes"].as_u64().unwrap_or(0))
    );
    println!(
        "  fit         {} — {}",
        plan["fit"]["label"].as_str().unwrap_or("-"),
        plan["fit"]["note"].as_str().unwrap_or("")
    );
    println!(
        "  checksum    {}",
        if plan["verifiable"].as_bool().unwrap_or(false) {
            "published by the source, will be verified"
        } else {
            "not published; size will be checked instead"
        }
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_is_a_fixed_width_at_every_fraction() {
        for fraction in [0.0, 0.25, 0.5, 0.999, 1.0] {
            assert_eq!(bar(fraction).chars().count(), 28, "fraction {fraction}");
        }
    }

    #[test]
    fn bar_fills_and_empties_completely() {
        assert!(bar(0.0).starts_with('─'));
        assert!(bar(1.0).chars().all(|c| c == '━'));
    }
}
