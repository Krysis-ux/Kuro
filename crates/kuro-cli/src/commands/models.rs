//! `kuro list`, `kuro ps`, `kuro rm`, `kuro stop`.

use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::client::{format_bytes, KuroClient};

pub async fn list(client: &KuroClient) -> Result<()> {
    client.ensure_running().await?;
    let response = client.get("/api/models").await?;
    let models = response["models"].as_array().cloned().unwrap_or_default();

    if models.is_empty() {
        println!("No models installed.");
        println!("\nSee what is recommended for this machine:");
        println!("    kuro recommended");
        return Ok(());
    }

    println!("MODEL                             SIZE      QUANT     STATUS    LOADED");
    for entry in models {
        let model = &entry["model"];
        println!(
            "{:<34}{:<10}{:<10}{:<10}{}",
            truncate(model["id"].as_str().unwrap_or("-"), 33),
            format_bytes(model["file_size_bytes"].as_u64().unwrap_or(0)),
            model["quant"].as_str().unwrap_or("-"),
            model["status"].as_str().unwrap_or("-"),
            if entry["loaded"].as_bool().unwrap_or(false) {
                "yes"
            } else {
                ""
            },
        );
    }

    Ok(())
}

/// Kuro's built-in suggestions, annotated with how well each fits this machine.
pub async fn recommended(client: &KuroClient) -> Result<()> {
    client.ensure_running().await?;
    let response = client.get("/api/models/recommended").await?;
    let models = response["models"].as_array().cloned().unwrap_or_default();

    println!("MODEL                 SIZE      PARAMS    FIT");
    for model in models {
        let installed = model["installed"].as_bool().unwrap_or(false);
        println!(
            "{:<22}{:<10}{:<10}{:<14}{}",
            truncate(model["slug"].as_str().unwrap_or("-"), 21),
            format_bytes(model["approxSizeBytes"].as_u64().unwrap_or(0)),
            model["paramCount"].as_str().unwrap_or("-"),
            model["fit"]["label"].as_str().unwrap_or("-"),
            if installed { "installed" } else { "" },
        );
    }

    println!("\nPull one with:  kuro pull <model>");
    Ok(())
}

/// Models currently held in memory.
pub async fn ps(client: &KuroClient) -> Result<()> {
    client.ensure_running().await?;
    let response = client.get("/api/models/loaded").await?;
    let loaded = response["loaded"].as_array().cloned().unwrap_or_default();

    if loaded.is_empty() {
        println!("No models are loaded.");
        return Ok(());
    }

    println!("MODEL                             PORT      PID       IDLE");
    for engine in loaded {
        println!(
            "{:<34}{:<10}{:<10}{}",
            truncate(engine["model_id"].as_str().unwrap_or("-"), 33),
            engine["port"].as_u64().unwrap_or(0),
            engine["pid"].as_u64().unwrap_or(0),
            format_duration(engine["idle_seconds"].as_i64().unwrap_or(0)),
        );
    }

    Ok(())
}

pub async fn remove(client: &KuroClient, model_id: &str) -> Result<()> {
    client.ensure_running().await?;
    client.delete(&format!("/api/models/{model_id}")).await?;
    println!("Removed {model_id}.");
    Ok(())
}

/// Unload a model from memory without deleting its weights.
pub async fn stop(client: &KuroClient, model_id: Option<&str>) -> Result<()> {
    client.ensure_running().await?;

    let targets = match model_id {
        Some(id) => vec![id.to_string()],
        None => {
            let response = client.get("/api/models/loaded").await?;
            let loaded: Vec<String> = response["loaded"]
                .as_array()
                .map(|engines| {
                    engines
                        .iter()
                        .filter_map(|engine| engine["model_id"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            if loaded.is_empty() {
                println!("No models are loaded.");
                return Ok(());
            }
            loaded
        }
    };

    for target in targets {
        let response: Value = client
            .post(&format!("/api/models/{target}/unload"), json!({}))
            .await?;
        if response["unloaded"].as_bool().unwrap_or(false) {
            println!("Stopped {target}.");
        } else {
            println!("{target} was not loaded.");
        }
    }

    Ok(())
}

/// Resolve which model to talk to when the user did not name one.
pub async fn resolve_model(client: &KuroClient, requested: Option<String>) -> Result<String> {
    if let Some(requested) = requested {
        return Ok(requested);
    }

    let response = client.get("/api/models").await?;
    let ready: Vec<String> = response["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter(|entry| entry["model"]["status"] == "ready")
                .filter_map(|entry| entry["model"]["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    match ready.len() {
        0 => bail!("No models are installed yet.\n\nPull one first, for example:\n    kuro pull qwen3-4b"),
        1 => Ok(ready.into_iter().next().expect("length checked")),
        _ => bail!(
            "Several models are installed, so one must be named:\n    {}",
            ready.join("\n    ")
        ),
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn format_duration(seconds: i64) -> String {
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s => format!("{}h{}m", s / 3600, (s % 3600) / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_identifiers_without_splitting_characters() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
        assert_eq!(truncate("日本語のモデル名", 4), "日本語…");
    }

    #[test]
    fn formats_idle_durations() {
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(600), "10m");
        assert_eq!(format_duration(7260), "2h1m");
    }
}
