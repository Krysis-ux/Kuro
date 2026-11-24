//! Long-running processes a workspace started (dev servers, watchers).
//!
//! A background process has an id, its output goes into a ring buffer the model
//! and the interface can both read, and the local address it prints on startup is
//! picked out of that output so the interface can point a preview at it.
//!
//! Processes belong to the daemon, not to a conversation: a dev server started in
//! one turn keeps serving in the next. They are killed when the daemon exits and
//! can be stopped explicitly at any time.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Lines of output kept per process (the recent ones).
const MAX_LOG_LINES: usize = 500;

/// Most processes one workspace may have running at once, so a model that keeps
/// starting servers cannot fill the machine.
pub const MAX_PER_WORKSPACE: usize = 6;

/// A process the model started and left running.
#[derive(Debug, Clone, Serialize)]
pub struct RunningProcess {
    pub id: String,
    pub workspace_id: String,
    pub command: String,
    pub pid: Option<u32>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// The local address the process printed on startup, if any (the preview target).
    pub url: Option<String>,
    pub running: bool,
    pub exit_code: Option<i32>,
}

/// The mutable half, shared with the tasks reading the process's output.
#[derive(Debug, Default)]
struct Shared {
    log: VecDeque<String>,
    url: Option<String>,
    running: bool,
    exit_code: Option<i32>,
}

struct Entry {
    id: String,
    workspace_id: String,
    command: String,
    pid: Option<u32>,
    started_at: chrono::DateTime<chrono::Utc>,
    shared: Arc<Mutex<Shared>>,
    /// Asks the process's own task to kill it. A channel, not a shared `Child`:
    /// waiting needs `&mut`, so a lock around the child would be held by the
    /// waiter for the process's whole life and `stop` would deadlock.
    stop: tokio::sync::watch::Sender<bool>,
}

impl Entry {
    fn snapshot(&self) -> RunningProcess {
        let shared = self.shared.lock().expect("process state lock");
        RunningProcess {
            id: self.id.clone(),
            workspace_id: self.workspace_id.clone(),
            command: self.command.clone(),
            pid: self.pid,
            started_at: self.started_at,
            url: shared.url.clone(),
            running: shared.running,
            exit_code: shared.exit_code,
        }
    }
}

/// Every background process the daemon is holding.
#[derive(Clone, Default)]
pub struct ProcessRegistry {
    entries: Arc<Mutex<Vec<Entry>>>,
}

impl std::fmt::Debug for ProcessRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessRegistry")
            .field("count", &self.list_all().len())
            .finish()
    }
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start an already-vetted command and leave it running. Both output streams
    /// are drained continuously, since a process with a full pipe stalls.
    pub async fn start(
        &self,
        workspace_id: &str,
        root: &Path,
        command: &str,
    ) -> Result<RunningProcess, String> {
        self.reap();

        if self.list(workspace_id).iter().filter(|held| held.running).count() >= MAX_PER_WORKSPACE {
            return Err(format!(
                "this workspace already has {MAX_PER_WORKSPACE} processes running. Stop one \
                 before starting another."
            ));
        }

        let (program, flag) = super::exec::shell();

        let mut builder = tokio::process::Command::new(program);
        builder
            .arg(flag)
            .arg(command)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .kill_on_drop(true);
        // Stopping `npm run dev` must kill the whole tree, not just the shell.
        super::exec::own_group(&mut builder);

        let mut child = builder
            .spawn()
            .map_err(|error| format!("could not start `{command}`: {error}"))?;

        let pid = child.id();
        let shared = Arc::new(Mutex::new(Shared {
            running: true,
            ..Shared::default()
        }));

        if let Some(stdout) = child.stdout.take() {
            spawn_reader(BufReader::new(stdout), shared.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(BufReader::new(stderr), shared.clone());
        }

        let id = uuid::Uuid::new_v4().to_string();
        let (stop, mut stop_signal) = tokio::sync::watch::channel(false);

        // One task owns the child: it waits for it and is the only thing that
        // kills it, so no lock is needed and exit status updates without polling.
        {
            let shared = shared.clone();
            let command = command.to_string();
            let mut child = child;
            tokio::spawn(async move {
                let status = tokio::select! {
                    status = child.wait() => status,
                    _ = stop_signal.changed() => {
                        // The group first (bundler and workers), then the shell
                        // so `wait` returns.
                        if let Some(pid) = pid {
                            super::exec::kill_group(pid);
                        }
                        let _ = child.start_kill();
                        child.wait().await
                    }
                };

                let mut state = shared.lock().expect("process state lock");
                state.running = false;
                state.exit_code = status.ok().and_then(|status| status.code());
                tracing::debug!(%command, code = ?state.exit_code, "background process ended");
            });
        }

        let entry = Entry {
            id: id.clone(),
            workspace_id: workspace_id.to_string(),
            command: command.to_string(),
            pid,
            started_at: chrono::Utc::now(),
            shared,
            stop,
        };
        let snapshot = entry.snapshot();
        self.entries.lock().expect("registry lock").push(entry);

        Ok(snapshot)
    }

    /// Wait up to `wait` for a just-started process to print an address (or exit).
    pub async fn settle(&self, id: &str, wait: std::time::Duration) -> Option<RunningProcess> {
        let deadline = std::time::Instant::now() + wait;

        loop {
            let found = self.get(id)?;
            if found.url.is_some() || !found.running || std::time::Instant::now() >= deadline {
                return Some(found);
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }

    pub fn get(&self, id: &str) -> Option<RunningProcess> {
        self.entries
            .lock()
            .expect("registry lock")
            .iter()
            .find(|entry| entry.id == id)
            .map(Entry::snapshot)
    }

    pub fn list(&self, workspace_id: &str) -> Vec<RunningProcess> {
        self.entries
            .lock()
            .expect("registry lock")
            .iter()
            .filter(|entry| entry.workspace_id == workspace_id)
            .map(Entry::snapshot)
            .collect()
    }

    pub fn list_all(&self) -> Vec<RunningProcess> {
        self.entries
            .lock()
            .expect("registry lock")
            .iter()
            .map(Entry::snapshot)
            .collect()
    }

    /// Recent output, newest last.
    pub fn log(&self, id: &str, limit: usize) -> Option<Vec<String>> {
        let entries = self.entries.lock().expect("registry lock");
        let entry = entries.iter().find(|entry| entry.id == id)?;
        let shared = entry.shared.lock().expect("process state lock");
        let skip = shared.log.len().saturating_sub(limit);
        Some(shared.log.iter().skip(skip).cloned().collect())
    }

    /// Request a stop; returns false when there was nothing to stop. Does not wait
    /// for the process to die (the list is polled, so the row updates itself).
    pub fn stop(&self, id: &str) -> bool {
        let entries = self.entries.lock().expect("registry lock");
        let Some(entry) = entries.iter().find(|entry| entry.id == id) else {
            return false;
        };
        if !entry.shared.lock().expect("process state lock").running {
            return false;
        }

        // A send failure means the process already exited.
        entry.stop.send(true).is_ok()
    }

    /// Drop a finished process from the list. A running one is never forgotten
    /// (its port would be untracked); returns false if unknown or still running.
    pub fn forget(&self, id: &str) -> bool {
        let mut entries = self.entries.lock().expect("registry lock");
        let Some(position) = entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        if entries[position].shared.lock().expect("process state lock").running {
            return false;
        }
        entries.remove(position);
        true
    }

    /// Drop every finished process of one workspace; returns how many went.
    pub fn forget_finished(&self, workspace_id: &str) -> usize {
        let mut entries = self.entries.lock().expect("registry lock");
        let before = entries.len();
        entries.retain(|entry| {
            entry.workspace_id != workspace_id
                || entry.shared.lock().expect("process state lock").running
        });
        before - entries.len()
    }

    /// Stop everything belonging to one workspace.
    pub fn stop_all(&self, workspace_id: &str) {
        for held in self.list(workspace_id) {
            self.stop(&held.id);
        }
    }

    /// Prune exited processes once the list grows past 32. Not done on exit, so a
    /// recently failed build stays readable.
    fn reap(&self) {
        let mut entries = self.entries.lock().expect("registry lock");
        if entries.len() < 32 {
            return;
        }
        entries.retain(|entry| entry.shared.lock().expect("process state lock").running);
    }
}

/// Read one stream into the ring buffer, looking for an address as it goes.
fn spawn_reader<R>(reader: BufReader<R>, shared: Arc<Mutex<Shared>>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let found = detect_url(&line);
            let mut state = shared.lock().expect("process state lock");

            if state.url.is_none() {
                if let Some(url) = found {
                    state.url = Some(url);
                }
            }

            state.log.push_back(line);
            if state.log.len() > MAX_LOG_LINES {
                state.log.pop_front();
            }
        }
    });
}

/// Pick a local server address out of a line of output. Only local addresses
/// count: banners often link to the tool's docs.
pub fn detect_url(line: &str) -> Option<String> {
    let mut best: Option<String> = None;

    for (index, _) in line.match_indices("http") {
        let rest = &line[index..];
        if !rest.starts_with("http://") && !rest.starts_with("https://") {
            continue;
        }

        let end = rest
            .find(|character: char| character.is_whitespace() || matches!(character, '"' | '\'' | ',' | ')' | '>'))
            .unwrap_or(rest.len());
        let candidate = rest[..end].trim_end_matches(['.', ':', ';']);

        if !is_local(candidate) {
            continue;
        }
        // Keep the first: servers print loopback before the network address.
        if best.is_none() {
            best = Some(candidate.to_string());
        }
    }

    best
}

fn is_local(url: &str) -> bool {
    let without_scheme = url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let host = without_scheme
        .split(['/', ':'])
        .next()
        .unwrap_or(without_scheme);

    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0" | "[::1]" | "::1")
        || host.starts_with("192.168.")
        || host.starts_with("10.")
        || host.ends_with(".localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dev_servers_address_is_picked_out_of_its_banner() {
        assert_eq!(
            detect_url("  ➜  Local:   http://localhost:5173/"),
            Some("http://localhost:5173/".to_string())
        );
        assert_eq!(
            detect_url("Server running at http://127.0.0.1:3000"),
            Some("http://127.0.0.1:3000".to_string())
        );
        assert_eq!(
            detect_url("Listening on http://0.0.0.0:8080, press Ctrl+C"),
            Some("http://0.0.0.0:8080".to_string())
        );
    }

    #[test]
    fn a_link_to_documentation_is_not_mistaken_for_the_server() {
        // Vite prints its own homepage in the same banner as the local address.
        assert_eq!(detect_url("  ➜  press h to show help, see https://vitejs.dev"), None);
        assert_eq!(detect_url("Read more at https://nextjs.org/docs"), None);
    }

    #[test]
    fn a_line_with_no_address_yields_nothing() {
        assert_eq!(detect_url("Compiled successfully in 412ms"), None);
        assert_eq!(detect_url(""), None);
    }

    #[test]
    fn trailing_punctuation_is_not_part_of_the_address() {
        assert_eq!(
            detect_url("open http://localhost:4000."),
            Some("http://localhost:4000".to_string())
        );
    }

    #[tokio::test]
    async fn a_process_is_tracked_from_start_to_exit() {
        let registry = ProcessRegistry::new();
        let root = std::env::temp_dir();

        let started = registry
            .start("w1", &root, "echo http://localhost:9999 && sleep 0.2")
            .await
            .expect("started");

        assert!(started.running);
        assert_eq!(registry.list("w1").len(), 1);
        assert!(registry.list("w2").is_empty(), "processes belong to one workspace");

        let settled = registry
            .settle(&started.id, std::time::Duration::from_secs(3))
            .await
            .expect("still known");
        assert_eq!(settled.url.as_deref(), Some("http://localhost:9999"));

        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        let finished = registry.get(&started.id).expect("still known");
        assert!(!finished.running, "an exited process should not report as running");
        assert_eq!(finished.exit_code, Some(0));

        let log = registry.log(&started.id, 10).expect("log");
        assert!(log.iter().any(|line| line.contains("localhost:9999")));
    }

    #[tokio::test]
    async fn a_process_can_be_stopped_after_it_has_settled() {
        // The pause matters: a deadlock between `stop` and the waiting task only
        // shows once the waiter has been scheduled. The timeout turns a deadlock
        // into a failure.
        let registry = ProcessRegistry::new();
        let started = registry
            .start("w1", &std::env::temp_dir(), "sleep 60")
            .await
            .expect("started");

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let stopped = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            async { registry.stop(&started.id) },
        )
        .await
        .expect("stop must not block on the task that is waiting for the process");
        assert!(stopped);

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(!registry.get(&started.id).expect("known").running);
    }

    #[tokio::test]
    async fn stopping_something_that_is_not_running_says_so_rather_than_hanging() {
        let registry = ProcessRegistry::new();
        assert!(!registry.stop("not-a-process"));

        let finished = registry
            .start("w1", &std::env::temp_dir(), "true")
            .await
            .expect("started");
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        assert!(
            !registry.stop(&finished.id),
            "a process that already exited is not something there was anything to stop"
        );
    }

    #[tokio::test]
    async fn a_workspace_cannot_fill_the_machine_with_servers() {
        let registry = ProcessRegistry::new();
        let root = std::env::temp_dir();

        for _ in 0..MAX_PER_WORKSPACE {
            registry.start("w1", &root, "sleep 30").await.expect("started");
        }

        let refused = registry.start("w1", &root, "sleep 30").await;
        assert!(refused.is_err(), "the limit should hold");

        registry.stop_all("w1");
    }
}
