//! Long-running processes a workspace started.
//!
//! `npm test` finishes. `npm run dev` does not, and it is the more interesting
//! of the two: it is the one that produces something to look at. A model that
//! can only run commands that terminate can check whether code compiles but
//! never whether the page renders.
//!
//! So a background process is a first-class thing here rather than a command
//! that happens not to have exited. It has an id, its output is kept in a ring
//! buffer both the model and the interface can read, and the address it printed
//! on startup is picked out of that output so the interface can point a frame at
//! it. That last part is what makes "look at it" possible: a dev server
//! announces `http://localhost:5173`, and nothing else in the system would know
//! that unless the line were read.
//!
//! ## Lifetime
//!
//! Processes belong to the daemon, not to a conversation. A dev server started
//! in one turn is still serving in the next, which is the behaviour anyone would
//! expect and the only one that makes iterating on a page possible. They are
//! killed when the daemon exits, and can be stopped explicitly at any point.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Lines of output kept per process.
///
/// A dev server rebuilding on every keystroke produces a great deal of it, and
/// the useful part is always the recent part.
const MAX_LOG_LINES: usize = 500;

/// Most processes one workspace may have running at once.
///
/// A model that starts a server, forgets, and starts another has a bug, and the
/// symptom without a limit is a machine slowly filling up with orphaned node
/// processes.
pub const MAX_PER_WORKSPACE: usize = 6;

/// A process the model started and left running.
#[derive(Debug, Clone, Serialize)]
pub struct RunningProcess {
    pub id: String,
    pub workspace_id: String,
    pub command: String,
    pub pid: Option<u32>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// The address this process printed on startup, when it printed one. This is
    /// what the preview panel points at.
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
    /// Asks the process's own task to kill it.
    ///
    /// A channel rather than a shared handle to the `Child`, and that is not a
    /// style choice. Waiting on a child needs `&mut`, so any design where one
    /// task waits and another kills has to put the child behind a lock — and the
    /// waiting task then holds that lock for the entire lifetime of the process,
    /// which is exactly as long as a `stop` would have to wait for it. That
    /// deadlocks, and it deadlocks *intermittently*: whether it hangs depends on
    /// whether stop arrives before the waiter has been scheduled, so it survives
    /// a unit test and fails against a real dev server.
    ///
    /// Here one task owns the child outright and selects between waiting for it
    /// and being told to stop. Nothing is shared, so nothing can be held.
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

    /// Start a command and leave it running.
    ///
    /// The caller has already vetted the command; this only spawns it. Both
    /// output streams are read continuously rather than on demand, because a
    /// process whose pipe fills up stops making progress, and a dev server that
    /// mysteriously freezes after a few hundred lines is a worse bug than any it
    /// would have helped find.
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

        let mut child = tokio::process::Command::new(program)
            .arg(flag)
            .arg(command)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .kill_on_drop(true)
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

        // One task owns the child: it waits for it, and is also the only thing
        // that kills it. That turns "running" into "exited with 1" without
        // anybody polling, and leaves no handle for a second task to contend on.
        {
            let shared = shared.clone();
            let command = command.to_string();
            let mut child = child;
            tokio::spawn(async move {
                let status = tokio::select! {
                    status = child.wait() => status,
                    _ = stop_signal.changed() => {
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

    /// Wait briefly for a just-started process to announce an address.
    ///
    /// A dev server takes a second or two to bind a port, and a tool that
    /// returned before then would always answer "no URL yet" — which is useless
    /// to the model and leaves the preview panel blank on the one turn the user
    /// was watching. So the call waits, but only for as long as a server that is
    /// working would need.
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

    /// Stop a process. Returns false when there was nothing to stop.
    ///
    /// Returns as soon as the kill is requested rather than waiting for the
    /// process to die. A dev server takes a moment to shut down, and a request
    /// that blocked on it would make the stop button feel broken; the list is
    /// polled anyway, so the row updates on its own a moment later.
    pub fn stop(&self, id: &str) -> bool {
        let entries = self.entries.lock().expect("registry lock");
        let Some(entry) = entries.iter().find(|entry| entry.id == id) else {
            return false;
        };
        if !entry.shared.lock().expect("process state lock").running {
            return false;
        }

        // A send failure means the owning task is already gone, which means the
        // process has already exited — the same answer as "nothing to stop".
        entry.stop.send(true).is_ok()
    }

    /// Stop everything belonging to one workspace.
    pub fn stop_all(&self, workspace_id: &str) {
        for held in self.list(workspace_id) {
            self.stop(&held.id);
        }
    }

    /// Forget processes that have exited, so the list stays about what is live.
    ///
    /// Deliberately not automatic on exit: a build that failed thirty seconds ago
    /// is exactly what somebody wants to read, so a finished process survives
    /// until the next time something is started.
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

/// Pick a local server address out of a line of output.
///
/// Only local addresses count. A dev server's banner routinely contains a link
/// to its own documentation, and pointing the preview panel at vitejs.dev
/// because it appeared before the localhost line would be a maddening bug.
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
        // A server that reports both a loopback and a network address usually
        // prints loopback first, and that is the one guaranteed to be reachable
        // from this machine.
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

        // The address is read out of the output, not guessed.
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
        // The pause is the whole test. An earlier design put the child behind a
        // lock that the waiting task held for the process's entire lifetime, so
        // `stop` could only return if it won a race against that task being
        // scheduled — which it did, in a test with no pause, and never did
        // against a real dev server. Waiting first makes the deadlock certain
        // rather than occasional, and the timeout turns it into a failure
        // instead of a hung test run.
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
