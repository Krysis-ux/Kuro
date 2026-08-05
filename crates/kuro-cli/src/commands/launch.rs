//! `kuro launch` — run another coding tool against a Kuro model.
//!
//! The idea is Ollama's: you already have Claude Code installed and you already
//! like its interface, its file editing and its permission prompts. What you do
//! not necessarily have is a reason to send every keystroke to one company's
//! API. So Kuro stands in as the model — local weights, a free provider's
//! allowance, or a key you hold — and the tool it launches never knows.
//!
//! ## How this works, and what it is not
//!
//! Claude Code reads `ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN` from its
//! environment. So this sets both, points the first at the local daemon's
//! Anthropic-compatible endpoint, and execs the real binary. Nothing is
//! installed, nothing is patched, and no configuration file is edited — which
//! is deliberate. A launcher that rewrote a user's settings would leave them
//! broken the moment it crashed, and "undo whatever that did" is not a thing
//! anybody should have to work out. Close the terminal and the effect is gone.
//!
//! It is also not a way around anyone's terms. It runs a tool you installed,
//! against a model you supplied, on your machine.

use std::process::Command;

use anyhow::{bail, Result};

use crate::client::KuroClient;

const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// A tool Kuro knows how to point at itself.
struct Launchable {
    /// What the user types after `kuro launch`.
    name: &'static str,
    /// The executable to look for on the PATH.
    binary: &'static str,
    /// Which API shape it speaks, which decides the variables set for it.
    wire: Wire,
    /// Where to get it, for when it is not installed.
    install: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
enum Wire {
    /// Anthropic Messages, read from `ANTHROPIC_BASE_URL`.
    Anthropic,
    /// OpenAI, read from `OPENAI_BASE_URL`.
    OpenAi,
}

/// What can be launched today.
///
/// Short on purpose. Every entry here is one whose environment variables have
/// been checked against the tool's own documentation — a list of tools that
/// *might* work would be a list of ways to waste somebody's afternoon.
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

    // The daemon has to be up before anything is pointed at it, or the tool
    // starts and fails on its first message with a connection error that says
    // nothing about Kuro.
    client.ensure_running().await?;

    let Some(path) = which(target.binary) else {
        eprintln!("`{}` is not installed, or is not on your PATH.", target.binary);
        eprintln!("{DIM}Get it from {}{RESET}", target.install);
        bail!("`{}` not found", target.binary);
    };

    // Checked before launching rather than after: a model name that does not
    // resolve produces an error inside the other tool's interface, where it
    // looks like that tool is broken.
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
            // Kuro listens on loopback and has no authentication, so this is a
            // placeholder rather than a secret. It exists because the client
            // refuses to start without one.
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

    // Replaces this process, so the launched tool owns the terminal completely
    // — its own key handling, its own resize behaviour, its own exit code.
    // Anything less makes a full-screen interface feel subtly wrong.
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

/// Fail early on a model name Kuro cannot resolve.
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

/// Where a program is on the PATH, if it is.
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
        // A tool that is not installed and does not say where it comes from is
        // a dead end.
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
        // And something every system has, is.
        assert!(which("sh").is_some());
    }
}
