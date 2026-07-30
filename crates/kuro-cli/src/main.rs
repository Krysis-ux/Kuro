//! `kuro` — the Kuro LLM command line.
//!
//! Every command except `serve` is a client of the local daemon's HTTP API, the
//! same one the web interface uses.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod commands;

use client::KuroClient;

#[derive(Parser)]
#[command(
    name = "kuro",
    version,
    about = "Kuro LLM — run language models on your own machine",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the Kuro server.
    Serve {
        /// Port to listen on (default 8420).
        #[arg(long, short)]
        port: Option<u16>,
    },

    /// Download a model.
    ///
    /// Accepts a recommended name (`qwen3-4b`), a Hugging Face repository
    /// (`owner/repo`), or a pasted Hugging Face URL. Add `:QUANT` to choose a
    /// quantization, for example `qwen3-4b:Q5_K_M`.
    Pull { model: String },

    /// Show what a pull would download, without downloading it.
    Preview { model: String },

    /// List installed models.
    #[command(visible_alias = "ls")]
    List,

    /// Show the models Kuro recommends for this machine.
    #[command(visible_alias = "rec")]
    Recommended,

    /// Chat with a model.
    Run {
        /// Model to use. Optional when only one is installed.
        model: Option<String>,

        /// Send a single prompt and exit instead of starting a session.
        #[arg(trailing_var_arg = true)]
        prompt: Vec<String>,

        /// How much effort to spend: low, balanced, high or max.
        #[arg(long, short)]
        effort: Option<String>,
    },

    /// Show models currently loaded in memory.
    Ps,

    /// Show server, hardware and engine status.
    Status,

    /// Unload a model from memory, keeping its weights.
    Stop {
        /// Model to unload. Omit to unload everything.
        model: Option<String>,
    },

    /// Delete a model's weights from disk.
    Rm { model: String },
}

#[tokio::main]
async fn main() {
    if let Err(error) = dispatch().await {
        // `{error:#}` includes the anyhow context chain on one line.
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

async fn dispatch() -> Result<()> {
    let cli = Cli::parse();

    // `serve` replaces this process with the daemon and never returns, so it
    // must not construct a client first.
    if let Command::Serve { port } = cli.command {
        return commands::serve::serve(port);
    }

    let client = KuroClient::new()?;

    match cli.command {
        Command::Serve { .. } => unreachable!("handled above"),
        Command::Pull { model } => commands::pull::pull(&client, &model).await,
        Command::Preview { model } => commands::pull::preview(&client, &model).await,
        Command::List => commands::models::list(&client).await,
        Command::Recommended => commands::models::recommended(&client).await,
        Command::Run {
            model,
            prompt,
            effort,
        } => commands::run::run(&client, model, prompt, effort).await,
        Command::Ps => commands::models::ps(&client).await,
        Command::Status => commands::status::status(&client).await,
        Command::Stop { model } => commands::models::stop(&client, model.as_deref()).await,
        Command::Rm { model } => commands::models::remove(&client, &model).await,
    }
}
