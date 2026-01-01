//! Status command - show the status of the daemon

use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

use crate::cli::args::StatusArgs;

/// Default PID file path
fn default_pid_file() -> PathBuf {
    dirs::runtime_dir()
        .or_else(|| dirs::state_dir())
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

/// Status information
#[derive(Debug, Serialize)]
pub struct StatusInfo {
    /// Whether the daemon is running
    pub running: bool,
    /// Process ID if running
    pub pid: Option<u32>,
    /// PID file path
    pub pid_file: String,
    /// Uptime in seconds (if available)
    pub uptime_secs: Option<u64>,
    /// Active sockets (if available)
    pub sockets: Vec<SocketStatus>,
}

/// Socket status information
#[derive(Debug, Serialize)]
pub struct SocketStatus {
    /// Socket path
    pub path: String,
    /// Whether the socket file exists
    pub exists: bool,
    /// Number of active connections
    pub connections: u32,
}

/// Execute the status command
pub async fn execute(args: StatusArgs) -> Result<()> {
    let pid_file = args.pid_file.unwrap_or_else(default_pid_file);

    let status = get_status(&pid_file)?;

    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&status)?;
            println!("{}", json);
        }
        _ => {
            print_text_status(&status);
        }
    }

    Ok(())
}

/// Get status information
fn get_status(pid_file: &PathBuf) -> Result<StatusInfo> {
    let mut status = StatusInfo {
        running: false,
        pid: None,
        pid_file: pid_file.display().to_string(),
        uptime_secs: None,
        sockets: Vec::new(),
    };

    if !pid_file.exists() {
        return Ok(status);
    }

    // Read PID from file
    let pid_str = fs::read_to_string(pid_file).context("Failed to read PID file")?;
    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => return Ok(status),
    };

    status.pid = Some(pid);
    status.running = is_process_running(pid);

    if status.running {
        // Try to get uptime from /proc on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(stat) = fs::metadata(format!("/proc/{}", pid)) {
                if let Ok(created) = stat.created() {
                    if let Ok(elapsed) = created.elapsed() {
                        status.uptime_secs = Some(elapsed.as_secs());
                    }
                }
            }
        }

        // TODO: Get socket information from running daemon
        // This would require IPC with the daemon process
    }

    Ok(status)
}

/// Print status in text format
fn print_text_status(status: &StatusInfo) {
    println!("authsock-filter Status");
    println!("======================");
    println!();

    if status.running {
        println!("Status:   RUNNING");
        if let Some(pid) = status.pid {
            println!("PID:      {}", pid);
        }
        if let Some(uptime) = status.uptime_secs {
            println!("Uptime:   {}", format_uptime(uptime));
        }
    } else {
        println!("Status:   STOPPED");
        if status.pid.is_some() {
            println!("Note:     Stale PID file exists");
        }
    }

    println!("PID File: {}", status.pid_file);

    if !status.sockets.is_empty() {
        println!();
        println!("Sockets:");
        for socket in &status.sockets {
            let status_icon = if socket.exists { "[OK]" } else { "[--]" };
            println!(
                "  {} {} (connections: {})",
                status_icon, socket.path, socket.connections
            );
        }
    }
}

/// Format uptime in human-readable format
fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, minutes, seconds)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}
