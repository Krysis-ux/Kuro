
use std::process::Command;

use anyhow::{bail, Result};

use crate::client::KuroClient;

const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

struct Launchable {
    name: &'static str,
    binary: &'static str,
    wire: Wire,
    install: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
enum Wire {
    Anthropic,
    OpenAi,
}

const LAUNCHABLE: &[Launchable] = &[
    Launchable {
        name: "claude",
        binary: "claude",
        wire: Wire::Anthropic,
        install: "https://claude.com/claude-code",
    },
    Launchable {
        name: "codex",
        binary: "codex",
        wire: Wire::OpenAi,
        install: "https://github.com/openai/codex",
    },
    Launchable {
        name: "opencode",
        binary: "opencode",
        wire: Wire::OpenAi,
        install: "https://opencode.ai",
    },
];

pub async fn launch(
    client: &KuroClient,
    app: Option<String>,
    model: Option<String>,
    rest: Vec<String>,
) -> Result<()> {
    let Some(app) = app else {
        list();
        return Ok(());
    };

    let Some(target) = LAUNCHABLE.iter().find(|entry| entry.name == app) else {
        eprintln!("Kuro does not know how to launch `{app}`.");
        printly_list();
        bail!("unknown application");
    };

    client.ensure_running().await?;

    let Some(path) = which(target.binary) else {
        eprintln!("`{}` is not installed, or is not on your PATH.", target.binary);
        eprintln!("{DIM}Get it from {}{RESET}", target.install);
        bail!("`{}` not found", target.binary);
    };

    let model = match model {
        Some(name) => {
            verify_model(client, &name).await?;
            Some(name)
        }
        None => None,
    };

    let base = client.base_url();

    println!();
    println!("{BOLD}Launching {}{RESET}", target.name);
    println!("{DIM}  model     {}{RESET}", model.as_deref().unwrap_or("Kuro's default"));
    println!("{DIM}  through   {base}{RESET}");
    println!("{DIM}  binary    {path}{RESET}");
    println!();

    let mut command = Command::new(&path);
    command.args(&rest);

    match target.wire {
        Wire::Anthropic => {
            command.env("ANTHROPIC_BASE_URL", &base);
            command.env("ANTHROPIC_AUTH_TOKEN", "kuro-local");
            command.env("ANTHROPIC_API_KEY", "kuro-local");
            if let Some(model) = &model {
                command.env("ANTHROPIC_MODEL", model);
            }
        }
        Wire::OpenAi => {
            command.env("OPENAI_BASE_URL", format!("{base}/v1"));
            command.env("OPENAI_API_KEY", "kuro-local");
            if let Some(model) = &model {
                command.env("OPENAI_MODEL", model);
            }
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        bail!("could not run `{path}`: {error}");
    }

    #[cfg(not(unix))]
    {
        let status = command.status()?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

async fn verify_model(client: &KuroClient, model: &str) -> Result<()> {
    let listing = client.get("/api/models").await?;

    let local = listing["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .any(|entry| entry["model"]["id"].as_str() == Some(model))
        })
        .unwrap_or(false);
    let remote = listing["remote"]
        .as_array()
        .map(|models| models.iter().any(|entry| entry["id"].as_str() == Some(model)))
        .unwrap_or(false);

    if local || remote {
        return Ok(());
    }

    bail!("`{model}` is not a model Kuro knows. Run `kuro list` to see what is available.");
}

fn list() {
    println!();
    println!("{BOLD}kuro launch{RESET} runs a coding tool against a Kuro model.");
    printly_list();
    println!();
    println!("{DIM}  kuro launch claude{RESET}");
    println!("{DIM}  kuro launch claude --model free:coding{RESET}");
    println!();
}

fn printly_list() {
    println!();
    for entry in LAUNCHABLE {
        let installed = if which(entry.binary).is_some() {
            "installed"
        } else {
            "not installed"
        };
        println!("  {:<10} {DIM}{installed}{RESET}", entry.name);
    }
}

fn which(binary: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| is_executable(candidate))
        .map(|found| found.display().to_string())
}

fn is_executable(path: &std::path::Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_launchable_says_where_to_get_it() {
        for entry in LAUNCHABLE {
            assert!(entry.install.starts_with("https://"), "{}", entry.name);
            assert!(!entry.binary.is_empty());
        }
    }

    #[test]
    fn names_are_unique_so_a_launch_is_unambiguous() {
        let mut seen: Vec<&str> = Vec::new();
        for entry in LAUNCHABLE {
            assert!(!seen.contains(&entry.name), "duplicate `{}`", entry.name);
            seen.push(entry.name);
        }
    }

    #[test]
    fn a_program_that_is_not_on_the_path_is_not_found() {
        assert!(which("kuro-definitely-not-a-real-program").is_none());
        assert!(which("sh").is_some());
    }
}
