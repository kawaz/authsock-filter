//! Stop command - stop the running daemon

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::cli::args::StopArgs;

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
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Send a signal to a process
#[cfg(unix)]
fn send_signal(pid: u32, signal: i32) -> bool {
    unsafe { libc::kill(pid as i32, signal) == 0 }
}

/// Execute the stop command
pub async fn execute(args: StopArgs) -> Result<()> {
    let pid_file = args.pid_file.unwrap_or_else(default_pid_file);

    // Check if PID file exists
    if !pid_file.exists() {
        bail!(
            "PID file not found: {}. Is the daemon running?",
            pid_file.display()
        );
    }

    // Read PID from file
    let pid_str = fs::read_to_string(&pid_file).context("Failed to read PID file")?;
    let pid: u32 = pid_str.trim().parse().context("Invalid PID in file")?;

    // Check if process is running
    if !is_process_running(pid) {
        warn!("Process {} is not running. Cleaning up PID file.", pid);
        fs::remove_file(&pid_file).context("Failed to remove PID file")?;
        println!("Daemon was not running. PID file cleaned up.");
        return Ok(());
    }

    info!(pid = pid, "Stopping daemon...");

    #[cfg(unix)]
    {
        if args.force {
            // Send SIGKILL immediately
            info!("Sending SIGKILL to process {}", pid);
            if !send_signal(pid, libc::SIGKILL) {
                bail!("Failed to send SIGKILL to process {}", pid);
            }
        } else {
            // Send SIGTERM for graceful shutdown
            info!("Sending SIGTERM to process {}", pid);
            if !send_signal(pid, libc::SIGTERM) {
                bail!("Failed to send SIGTERM to process {}", pid);
            }

            // Wait for process to exit
            let timeout = Duration::from_secs(args.timeout);
            let poll_interval = Duration::from_millis(100);
            let mut elapsed = Duration::ZERO;

            while is_process_running(pid) && elapsed < timeout {
                sleep(poll_interval).await;
                elapsed += poll_interval;
            }

            // If still running, force kill
            if is_process_running(pid) {
                warn!(
                    "Process {} did not exit within {} seconds, sending SIGKILL",
                    pid, args.timeout
                );
                if !send_signal(pid, libc::SIGKILL) {
                    bail!("Failed to send SIGKILL to process {}", pid);
                }

                // Wait a bit more for SIGKILL to take effect
                sleep(Duration::from_millis(500)).await;

                if is_process_running(pid) {
                    bail!("Failed to stop process {} even with SIGKILL", pid);
                }
            }
        }

        // Remove PID file
        fs::remove_file(&pid_file).context("Failed to remove PID file")?;
    }

    #[cfg(not(unix))]
    {
        bail!("Stop command is only supported on Unix systems");
    }

    info!("Daemon stopped successfully");
    println!("Daemon stopped successfully");

    Ok(())
}
