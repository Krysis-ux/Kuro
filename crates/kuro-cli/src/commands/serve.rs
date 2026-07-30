//! `kuro serve` — start the daemon.

use std::path::PathBuf;

use anyhow::{bail, Result};

/// Hand this process over to the daemon binary.
///
/// Using `exec` rather than spawning a child means Ctrl-C, job control and the
/// exit status behave exactly as if the daemon had been started directly.
pub fn serve(port: Option<u16>) -> Result<()> {
    use std::os::unix::process::CommandExt;

    let binary = locate_daemon()?;
    let mut command = std::process::Command::new(&binary);
    if let Some(port) = port {
        command.env("KURO_PORT", port.to_string());
    }

    // `exec` only returns if it failed.
    let error = command.exec();
    bail!("could not start {}: {error}", binary.display())
}

/// Find `kuro-server`, preferring the copy shipped alongside this binary.
fn locate_daemon() -> Result<PathBuf> {
    if let Some(configured) = std::env::var_os("KURO_SERVER_BIN") {
        let path = PathBuf::from(configured);
        if path.exists() {
            return Ok(path);
        }
        bail!("KURO_SERVER_BIN points at {} which does not exist", path.display());
    }

    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let sibling = dir.join("kuro-server");
            if sibling.exists() {
                return Ok(sibling);
            }
        }
    }

    // Fall back to whatever is on PATH.
    if which("kuro-server") {
        return Ok(PathBuf::from("kuro-server"));
    }

    bail!(
        "could not find the `kuro-server` binary.\n\n\
         It normally sits next to `kuro`. If you are running from a source \
         checkout, build both with:\n    cargo build --release"
    )
}

fn which(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(program).exists())
}
