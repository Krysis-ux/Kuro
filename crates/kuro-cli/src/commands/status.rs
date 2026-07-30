//! `kuro status` — is the daemon up, and what is it doing?

use anyhow::Result;

use crate::client::{format_bytes, KuroClient};

pub async fn status(client: &KuroClient) -> Result<()> {
    client.ensure_running().await?;

    let status = client.get("/api/status").await?;
    let hardware = client.get("/api/hardware").await?;

    println!("Kuro LLM {}", status["version"].as_str().unwrap_or("?"));
    println!("  address    {}", status["address"].as_str().unwrap_or("-"));
    println!(
        "  uptime     {}",
        format_uptime(status["uptimeSeconds"].as_i64().unwrap_or(0))
    );
    println!(
        "  data       {}",
        status["dataDirectory"].as_str().unwrap_or("-")
    );

    let machine = &hardware["hardware"];
    println!();
    println!(
        "  chip       {}",
        machine["chip"].as_str().unwrap_or("unknown")
    );
    println!(
        "  memory     {}",
        format_bytes(machine["total_memory_bytes"].as_u64().unwrap_or(0))
    );
    println!(
        "  cores      {} physical",
        machine["physical_cores"].as_u64().unwrap_or(0)
    );
    println!(
        "  gpu        {}",
        if machine["gpu_available"].as_bool().unwrap_or(false) {
            machine["gpu_backend"].as_str().unwrap_or("available")
        } else {
            "cpu only"
        }
    );

    let engine = &hardware["effectiveEngineSettings"];
    println!();
    println!(
        "  context    {}",
        engine["context_size"].as_u64().unwrap_or(0)
    );
    println!("  gpu layers {}", engine["gpu_layers"].as_i64().unwrap_or(0));
    println!("  threads    {}", engine["threads"].as_u64().unwrap_or(0));

    let loaded = status["loadedModels"].as_array().cloned().unwrap_or_default();
    println!();
    if loaded.is_empty() {
        println!("  no models loaded");
    } else {
        for engine in loaded {
            println!(
                "  loaded     {} (port {})",
                engine["model_id"].as_str().unwrap_or("-"),
                engine["port"].as_u64().unwrap_or(0)
            );
        }
    }

    Ok(())
}

fn format_uptime(seconds: i64) -> String {
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {}s", s / 60, s % 60),
        s => format!("{}h {}m", s / 3600, (s % 3600) / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_uptime_at_each_scale() {
        assert_eq!(format_uptime(30), "30s");
        assert_eq!(format_uptime(125), "2m 5s");
        assert_eq!(format_uptime(7325), "2h 2m");
    }
}
