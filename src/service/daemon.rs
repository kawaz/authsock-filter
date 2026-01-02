//! Daemon management for authsock-filter
//!
//! Provides functionality to run authsock-filter as a background daemon:
//! - Start: Fork to background and create PID file
//! - Stop: Read PID file and send SIGTERM
//! - Status: Check if daemon is running

use crate::error::{Error, Result};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Daemon status information
#[derive(Debug, Clone)]
pub struct DaemonStatus {
    /// Whether the daemon is running
    pub running: bool,
    /// Process ID if running
    pub pid: Option<u32>,
    /// PID file path
    pub pid_file: PathBuf,
}

/// Daemon manager for authsock-filter
#[derive(Debug)]
pub struct Daemon {
    /// Path to the PID file
    pid_file: PathBuf,
}

impl Daemon {
    /// Create a new Daemon manager with default PID file location
    ///
    /// PID file location: $XDG_RUNTIME_DIR/authsock-filter/authsock-filter.pid
    /// Falls back to /tmp/authsock-filter/authsock-filter.pid if XDG_RUNTIME_DIR is not set
    pub fn new() -> Self {
        Self {
            pid_file: Self::default_pid_file(),
        }
    }

    /// Create a new Daemon manager with a custom PID file path
    pub fn with_pid_file(pid_file: PathBuf) -> Self {
        Self { pid_file }
    }

    /// Get the default PID file path
    pub fn default_pid_file() -> PathBuf {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));

        runtime_dir
            .join("authsock-filter")
            .join("authsock-filter.pid")
    }

    /// Get the PID file path
    pub fn pid_file(&self) -> &PathBuf {
        &self.pid_file
    }

    /// Start the daemon in the background
    ///
    /// This starts authsock-filter with the given arguments as a background process
    /// and creates a PID file.
    pub fn start(&self, args: &[String]) -> Result<u32> {
        // Check if already running
        if let Ok(status) = self.status()
            && status.running
        {
            return Err(Error::Daemon(format!(
                "Daemon is already running with PID {}",
                status.pid.unwrap_or(0)
            )));
        }

        // Ensure PID file directory exists
        if let Some(parent) = self.pid_file.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Error::Daemon(format!("Failed to create PID file directory: {}", e))
            })?;
        }

        // Get the path to the current executable
        let executable = std::env::current_exe()
            .map_err(|e| Error::Daemon(format!("Failed to get current executable path: {}", e)))?;

        // Build the command with "run" subcommand and provided arguments
        let mut cmd = Command::new(&executable);
        cmd.arg("run");
        cmd.args(args);

        // Detach from the current process
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        // Spawn the process
        let child = cmd
            .spawn()
            .map_err(|e| Error::Daemon(format!("Failed to start daemon process: {}", e)))?;

        let pid = child.id();

        // Write PID file
        fs::write(&self.pid_file, pid.to_string())
            .map_err(|e| Error::Daemon(format!("Failed to write PID file: {}", e)))?;

        tracing::info!(pid = pid, pid_file = %self.pid_file.display(), "Daemon started");

        Ok(pid)
    }

    /// Stop the running daemon
    ///
    /// Reads the PID file and sends SIGTERM to the process.
    pub fn stop(&self) -> Result<()> {
        let status = self.status()?;

        if !status.running {
            // Clean up stale PID file if it exists
            if self.pid_file.exists() {
                fs::remove_file(&self.pid_file).ok();
            }
            return Err(Error::Daemon("Daemon is not running".to_string()));
        }

        let pid = status
            .pid
            .ok_or_else(|| Error::Daemon("No PID found".to_string()))?;

        // Send SIGTERM to the process
        Self::send_signal(pid, "TERM")?;

        // Wait a bit for the process to terminate and then clean up PID file
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Remove PID file
        if self.pid_file.exists() {
            fs::remove_file(&self.pid_file).ok();
        }

        tracing::info!(pid = pid, "Daemon stopped");

        Ok(())
    }

    /// Check if the daemon is running
    ///
    /// Returns the daemon status including whether it's running and its PID.
    pub fn status(&self) -> Result<DaemonStatus> {
        if !self.pid_file.exists() {
            return Ok(DaemonStatus {
                running: false,
                pid: None,
                pid_file: self.pid_file.clone(),
            });
        }

        // Read PID from file
        let pid_str = fs::read_to_string(&self.pid_file)
            .map_err(|e| Error::Daemon(format!("Failed to read PID file: {}", e)))?;

        let pid: u32 = pid_str
            .trim()
            .parse()
            .map_err(|e| Error::Daemon(format!("Invalid PID in file: {}", e)))?;

        // Check if process is running
        let running = Self::is_process_running(pid);

        Ok(DaemonStatus {
            running,
            pid: Some(pid),
            pid_file: self.pid_file.clone(),
        })
    }

    /// Check if a process with the given PID is running
    #[cfg(unix)]
    fn is_process_running(pid: u32) -> bool {
        // Use kill -0 to check if process exists
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    fn is_process_running(_pid: u32) -> bool {
        // On non-Unix systems, we can't easily check
        false
    }

    /// Send a signal to a process
    #[cfg(unix)]
    fn send_signal(pid: u32, signal: &str) -> Result<()> {
        let output = Command::new("kill")
            .args([&format!("-{}", signal), &pid.to_string()])
            .output()
            .map_err(|e| Error::Daemon(format!("Failed to run kill command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Daemon(format!(
                "Failed to send {} to process {}: {}",
                signal,
                pid,
                stderr.trim()
            )));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn send_signal(_pid: u32, _signal: &str) -> Result<()> {
        Err(Error::Daemon(
            "Signal sending is only supported on Unix systems".to_string(),
        ))
    }

    /// Clean up stale PID file if the process is not running
    pub fn cleanup_stale_pid_file(&self) -> Result<bool> {
        if !self.pid_file.exists() {
            return Ok(false);
        }

        let status = self.status()?;
        if !status.running {
            fs::remove_file(&self.pid_file)
                .map_err(|e| Error::Daemon(format!("Failed to remove stale PID file: {}", e)))?;
            tracing::debug!(pid_file = %self.pid_file.display(), "Removed stale PID file");
            return Ok(true);
        }

        Ok(false)
    }
}

impl Default for Daemon {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_pid_file() {
        let pid_file = Daemon::default_pid_file();
        assert!(pid_file.ends_with("authsock-filter/authsock-filter.pid"));
    }

    #[test]
    fn test_daemon_with_custom_pid_file() {
        let custom_path = PathBuf::from("/tmp/test.pid");
        let daemon = Daemon::with_pid_file(custom_path.clone());
        assert_eq!(daemon.pid_file(), &custom_path);
    }

    #[test]
    fn test_status_no_pid_file() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("nonexistent.pid");
        let daemon = Daemon::with_pid_file(pid_file);

        let status = daemon.status().unwrap();
        assert!(!status.running);
        assert!(status.pid.is_none());
    }

    #[test]
    fn test_status_with_stale_pid() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("stale.pid");

        // Write a PID that doesn't exist (very high number)
        fs::write(&pid_file, "999999999").unwrap();

        let daemon = Daemon::with_pid_file(pid_file);
        let status = daemon.status().unwrap();

        assert!(!status.running);
        assert_eq!(status.pid, Some(999999999));
    }

    #[test]
    fn test_cleanup_stale_pid_file() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("stale.pid");

        // Write a stale PID
        fs::write(&pid_file, "999999999").unwrap();
        assert!(pid_file.exists());

        let daemon = Daemon::with_pid_file(pid_file.clone());
        let cleaned = daemon.cleanup_stale_pid_file().unwrap();

        assert!(cleaned);
        assert!(!pid_file.exists());
    }
}
