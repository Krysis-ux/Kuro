//! `kuro free` — the free-tier pool, from a terminal.
//!
//! The same two questions the Free models screen answers: which providers can
//! serve a request right now, and what they have cost so far. Reading it here
//! rather than opening a browser matters most in the case it is for — a machine
//! being set up over SSH, where the browser is somewhere else entirely.

use anyhow::Result;
use serde_json::Value;

use crate::client::KuroClient;

pub async fn free(client: &KuroClient) -> Result<()> {
    client.ensure_running().await?;

    let overview = client.get("/api/free").await?;
    let providers = overview["providers"].as_array().cloned().unwrap_or_default();

    let ready = overview["availableCount"].as_i64().unwrap_or(0);
    let keys = overview["keyCount"].as_i64().unwrap_or(0);
    let shared_allowed = overview["allowKeyless"].as_bool().unwrap_or(false);

    println!(
        "{keys} key{} stored, {ready} usable right now.",
        if keys == 1 { "" } else { "s" }
    );
    println!(
        "Shared endpoints: {}",
        if shared_allowed { "on" } else { "off" }
    );

    // Only the rows that mean something. A list of seventeen providers the user
    // has no key for is the web page's job, where it is scannable.
    let interesting: Vec<&Value> = providers
        .iter()
        .filter(|entry| {
            entry["hasKey"].as_bool().unwrap_or(false) || entry["keyless"].as_bool().unwrap_or(false)
        })
        .collect();

    if interesting.is_empty() {
        println!("\nNo providers are set up yet. Add a key on the Free models screen,");
        println!("or turn on the shared endpoints there to use Kuro Free with no account.");
        return Ok(());
    }

    println!("\nPROVIDER                  TIER            STATUS");
    for entry in &interesting {
        let trouble = entry["trouble"].as_str();
        let status = match (trouble, entry["expired"].as_bool().unwrap_or(false)) {
            (Some("rate_limited"), _) => "out of allowance",
            (Some("rejected"), _) => "key refused",
            (Some("model_gone"), _) => "rechecking models",
            (Some(_), _) => "unavailable",
            (None, true) => "trial ended",
            (None, false) => "ready",
        };

        println!(
            "{:<26}{:<16}{}",
            truncate(entry["name"].as_str().unwrap_or("-"), 25),
            entry["tier"].as_str().unwrap_or("-"),
            status
        );
    }

    report_usage(client).await
}

/// What the keys have actually cost, this month.
///
/// A separate request because it is a separate question, and the one whose cost
/// grows with the size of the message history.
async fn report_usage(client: &KuroClient) -> Result<()> {
    let usage = client.get("/api/free/usage").await?;
    let month = &usage["month"];
    let rows = month["providers"].as_array().cloned().unwrap_or_default();

    if rows.is_empty() {
        return Ok(());
    }

    println!("\nTHIS MONTH                TOKENS      MESSAGES");
    for row in &rows {
        println!(
            "{:<26}{:<12}{}",
            truncate(row["name"].as_str().unwrap_or("-"), 25),
            row["totalTokens"].as_i64().unwrap_or(0),
            row["turns"].as_i64().unwrap_or(0),
        );
    }

    // Said rather than hidden: several free tiers return no counts, so the
    // totals above are a floor and the reader should know by how much.
    let unreported = month["unreportedTurns"].as_i64().unwrap_or(0);
    if unreported > 0 {
        println!(
            "\n{unreported} message{} returned no token counts and {} not included.",
            if unreported == 1 { "" } else { "s" },
            if unreported == 1 { "is" } else { "are" }
        );
    }

    Ok(())
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    format!("{}…", text.chars().take(limit.saturating_sub(1)).collect::<String>())
}
