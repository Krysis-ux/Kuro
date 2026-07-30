//! Launching and stopping a single `llama-server` process.
//!
//! Each loaded model gets its own process. Running the engine out-of-process
//! means a model that crashes the engine — a malformed GGUF, an unsupported
//! architecture, an out-of-memory kill — takes down only that engine, never
//! Kuro itself.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::process::{Child, Command};

use crate::{KuroError, Result};

/// How long to wait for an engine to report healthy before giving up.
/// Large models on a cold page cache genuinely take tens of seconds.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(180);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How long a process gets to exit after SIGTERM before it is killed.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct EngineLaunchSpec {
    pub binary: PathBuf,
    pub model_path: PathBuf,
    /// Reported by the engine's own `/v1/models`, which keeps its responses
    /// consistent with Kuro's model ids.
    pub model_alias: String,
    pub port: u16,
    pub context_size: u32,
    pub gpu_layers: i32,
    pub threads: u32,
    pub log_path: PathBuf,
}

impl EngineLaunchSpec {
    /// Command-line arguments for `llama-server`.
    pub fn arguments(&self) -> Vec<String> {
        vec![
            "--model".to_string(),
            self.model_path.to_string_lossy().to_string(),
            "--alias".to_string(),
            self.model_alias.clone(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            self.port.to_string(),
            "--ctx-size".to_string(),
            self.context_size.to_string(),
            "--gpu-layers".to_string(),
            self.gpu_layers.to_string(),
            "--threads".to_string(),
            self.threads.to_string(),
            // Kuro serves its own interface; the engine's bundled one would
            // only be reachable on an internal port nobody is told about.
            "--no-webui".to_string(),
            // Use the chat template baked into the GGUF. Without this the engine
            // falls back to a generic template and ignores the `tools` field
            // entirely, so every tool call silently becomes prose describing a
            // tool call. Tool support is not optional enough to leave off.
            "--jinja".to_string(),
        ]
    }
}

/// Start an engine process with its output captured to a log file.
pub fn spawn_engine(spec: &EngineLaunchSpec) -> Result<Child> {
    if !spec.binary.exists() {
        return Err(KuroError::engine(format!(
            "engine binary is missing at {}",
            spec.binary.display()
        )));
    }
    if !spec.model_path.exists() {
        return Err(KuroError::engine(format!(
            "model file is missing at {}",
            spec.model_path.display()
        )));
    }

    if let Some(parent) = spec.log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log_file = std::fs::File::create(&spec.log_path)?;
    let log_file_for_stderr = log_file.try_clone()?;

    let child = Command::new(&spec.binary)
        .args(spec.arguments())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_for_stderr))
        .stdin(Stdio::null())
        // Reap the process ourselves so a crash is observed rather than
        // leaving a zombie.
        .kill_on_drop(false)
        .spawn()
        .map_err(|error| {
            KuroError::engine(format!("could not start the inference engine: {error}"))
        })?;

    Ok(child)
}

/// Poll the engine's health endpoint until it is ready to serve.
///
/// `llama-server` answers 503 while it is still loading weights and 200 once it
/// can accept requests.
pub async fn wait_until_healthy(
    client: &reqwest::Client,
    port: u16,
    log_path: &std::path::Path,
) -> Result<()> {
    let url = format!("http://127.0.0.1:{port}/health");
    let deadline = Instant::now() + HEALTH_TIMEOUT;

    loop {
        if let Ok(response) = client.get(&url).send().await {
            if response.status().is_success() {
                return Ok(());
            }
        }

        if Instant::now() >= deadline {
            return Err(KuroError::engine(format!(
                "the engine did not become ready within {} seconds. Last log lines:\n{}",
                HEALTH_TIMEOUT.as_secs(),
                tail_log(log_path, 15)
            )));
        }

        tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
    }
}

/// Stop a process: ask politely, then insist.
pub async fn terminate(child: &mut Child) {
    let Some(pid) = child.id() else {
        // Already reaped.
        return;
    };

    // SAFETY: `kill` with a pid we spawned and still hold a handle to.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }

    let graceful = tokio::time::timeout(SHUTDOWN_GRACE, child.wait()).await;
    if graceful.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}

/// Last `lines` lines of a log file, for error messages.
///
/// Engine failures are almost always explained in its own output, so surfacing
/// the tail turns "it did not start" into something actionable.
pub fn tail_log(path: &std::path::Path, lines: usize) -> String {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return "(no engine log available)".to_string();
    };

    let collected: Vec<&str> = contents.lines().collect();
    let start = collected.len().saturating_sub(lines);
    let tail = collected[start..].join("\n");

    if tail.trim().is_empty() {
        "(engine produced no output)".to_string()
    } else {
        tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> EngineLaunchSpec {
        EngineLaunchSpec {
            binary: PathBuf::from("/tmp/llama-server"),
            model_path: PathBuf::from("/tmp/model.gguf"),
            model_alias: "qwen3-4b:q4_k_m".to_string(),
            port: 39200,
            context_size: 4096,
            gpu_layers: 999,
            threads: 10,
            log_path: PathBuf::from("/tmp/engine.log"),
        }
    }

    #[test]
    fn builds_the_expected_command_line() {
        let arguments = spec().arguments();
        let joined = arguments.join(" ");

        assert!(joined.contains("--model /tmp/model.gguf"));
        assert!(joined.contains("--port 39200"));
        assert!(joined.contains("--host 127.0.0.1"), "engines must not be exposed directly");
        assert!(joined.contains("--ctx-size 4096"));
        assert!(joined.contains("--gpu-layers 999"));
        assert!(joined.contains("--threads 10"));
        assert!(joined.contains("--no-webui"));
        assert!(joined.contains("--alias qwen3-4b:q4_k_m"));
        assert!(
            joined.contains("--jinja"),
            "without --jinja the engine ignores tool definitions"
        );
    }

    #[test]
    fn refuses_to_launch_without_a_binary_or_model() {
        let mut broken = spec();
        broken.binary = PathBuf::from("/nonexistent/llama-server");
        let error = spawn_engine(&broken).unwrap_err().to_string();
        assert!(error.contains("engine binary is missing"), "got: {error}");
    }

    #[test]
    fn tail_of_a_missing_log_is_descriptive_not_a_panic() {
        let tail = tail_log(std::path::Path::new("/nonexistent/engine.log"), 5);
        assert!(tail.contains("no engine log"));
    }

    #[test]
    fn tail_returns_only_the_last_lines() {
        let path = std::env::temp_dir().join(format!("kuro-log-{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&path, "one\ntwo\nthree\nfour\n").expect("write");

        let tail = tail_log(&path, 2);

        assert_eq!(tail, "three\nfour");
        std::fs::remove_file(&path).ok();
    }
}
