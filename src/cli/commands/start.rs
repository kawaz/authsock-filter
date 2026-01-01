//! Start command - start the proxy as a background daemon

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{info, warn};

use crate::cli::args::StartArgs;

/// Default PID file path
fn default_pid_file() -> PathBuf {
    dirs::runtime_dir()
        .or(dirs::state_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("authsock-filter.pid")
}

/// Check if a process is running
fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Use kill -0 to check if process exists
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        // On non-Unix systems, assume process is running
        let _ = pid;
        true
    }
}

/// Execute the start command
pub async fn execute(args: StartArgs) -> Result<()> {
    let pid_file = args.pid_file.unwrap_or_else(default_pid_file);

    // Check if already running
    if pid_file.exists() {
        let pid_str = fs::read_to_string(&pid_file).context("Failed to read PID file")?;
        let pid: u32 = pid_str.trim().parse().context("Invalid PID in file")?;

        if is_process_running(pid) {
            bail!(
                "Daemon is already running (PID: {}). Use 'stop' first.",
                pid
            );
        } else {
            warn!("Stale PID file found, removing...");
            fs::remove_file(&pid_file).context("Failed to remove stale PID file")?;
        }
    }

    // Validate upstream socket
    let upstream = args
        .upstream
        .as_ref()
        .context("Upstream socket path is required. Set SSH_AUTH_SOCK or use --upstream")?;

    if !upstream.exists() {
        bail!("Upstream socket does not exist: {}", upstream.display());
    }

    info!("Starting daemon...");

    // Build command arguments for the run command
    let current_exe = std::env::current_exe().context("Failed to get current executable")?;

    let mut cmd = Command::new(&current_exe);
    cmd.arg("run");
    cmd.arg("--upstream").arg(upstream);

    if let Some(log) = &args.log {
        cmd.arg("--log").arg(log);
    }

    for socket in &args.sockets {
        cmd.arg("--socket").arg(socket);
    }

    // Daemonize the process
    #[cfg(unix)]
    {
        // Create a new session
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        // Fork and exec
        // Note: In a real implementation, we would use fork() properly
        // For now, we spawn and detach
        let child = cmd.spawn().context("Failed to spawn daemon process")?;

        let pid = child.id();

        // Ensure PID file directory exists
        if let Some(parent) = pid_file.parent() {
            fs::create_dir_all(parent).context("Failed to create PID file directory")?;
        }

        // Write PID file
        fs::write(&pid_file, pid.to_string()).context("Failed to write PID file")?;

        info!(pid = pid, pid_file = %pid_file.display(), "Daemon started");

        // Forget the child so it continues running
        std::mem::forget(child);
    }

    #[cfg(not(unix))]
    {
        bail!("Daemon mode is only supported on Unix systems");
    }

    println!("Daemon started successfully");
    println!("PID file: {}", pid_file.display());

    Ok(())
}
