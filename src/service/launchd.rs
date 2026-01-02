//! macOS launchd integration
//!
//! Provides functionality to register authsock-filter as a launchd user agent:
//! - Generate plist XML configuration
//! - Register with launchctl load
//! - Unregister with launchctl unload

use crate::error::{Error, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Service identifier for launchd
const SERVICE_LABEL: &str = "com.github.kawaz.authsock-filter";

/// Launchd manager for macOS
#[derive(Debug)]
pub struct Launchd {
    /// Path to the plist file
    plist_path: PathBuf,
    /// Service label
    label: String,
}

impl Launchd {
    /// Create a new Launchd manager with default plist location
    ///
    /// Plist location: ~/Library/LaunchAgents/com.github.kawaz.authsock-filter.plist
    pub fn new() -> Self {
        Self {
            plist_path: Self::default_plist_path(),
            label: SERVICE_LABEL.to_string(),
        }
    }

    /// Create a new Launchd manager with a custom plist path
    pub fn with_plist_path(plist_path: PathBuf) -> Self {
        Self {
            plist_path,
            label: SERVICE_LABEL.to_string(),
        }
    }

    /// Get the default plist path
    pub fn default_plist_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", SERVICE_LABEL))
    }

    /// Get the plist file path
    pub fn plist_path(&self) -> &PathBuf {
        &self.plist_path
    }

    /// Get the service label
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Generate the plist XML content
    ///
    /// # Arguments
    /// * `args` - Additional arguments to pass to authsock-filter run command
    pub fn generate_plist(&self, args: &[String]) -> Result<String> {
        let executable = std::env::current_exe()
            .map_err(|e| Error::Daemon(format!("Failed to get current executable path: {}", e)))?;

        let executable_path = executable.to_string_lossy();

        // Build program arguments array
        let mut program_args = vec![
            format!("        <string>{}</string>", executable_path),
            "        <string>run</string>".to_string(),
        ];
        for arg in args {
            program_args.push(format!("        <string>{}</string>", escape_xml(arg)));
        }
        let program_args_str = program_args.join("\n");

        // Standard output/error log paths
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join("Library")
            .join("Logs")
            .join("authsock-filter");
        let stdout_log = log_dir.join("authsock-filter.log");
        let stderr_log = log_dir.join("authsock-filter.error.log");

        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
{program_args}
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{stdout}</string>
    <key>StandardErrorPath</key>
    <string>{stderr}</string>
    <key>ProcessType</key>
    <string>Background</string>
</dict>
</plist>
"#,
            label = self.label,
            program_args = program_args_str,
            stdout = stdout_log.display(),
            stderr = stderr_log.display(),
        );

        Ok(plist)
    }

    /// Register the service with launchd
    ///
    /// This generates the plist file and loads it with launchctl.
    pub fn register(&self, args: &[String]) -> Result<()> {
        // Check if already registered
        if self.is_registered() {
            return Err(Error::Daemon(format!(
                "Service {} is already registered",
                self.label
            )));
        }

        // Ensure LaunchAgents directory exists
        if let Some(parent) = self.plist_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Error::Daemon(format!("Failed to create LaunchAgents directory: {}", e))
            })?;
        }

        // Ensure log directory exists
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join("Library")
            .join("Logs")
            .join("authsock-filter");
        fs::create_dir_all(&log_dir)
            .map_err(|e| Error::Daemon(format!("Failed to create log directory: {}", e)))?;

        // Generate and write plist
        let plist_content = self.generate_plist(args)?;
        fs::write(&self.plist_path, &plist_content)
            .map_err(|e| Error::Daemon(format!("Failed to write plist file: {}", e)))?;

        tracing::debug!(plist_path = %self.plist_path.display(), "Wrote plist file");

        // Load with launchctl
        let output = Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&self.plist_path)
            .output()
            .map_err(|e| Error::Daemon(format!("Failed to run launchctl load: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Clean up plist file on failure
            fs::remove_file(&self.plist_path).ok();
            return Err(Error::Daemon(format!(
                "launchctl load failed: {}",
                stderr.trim()
            )));
        }

        tracing::info!(
            label = %self.label,
            plist_path = %self.plist_path.display(),
            "Service registered with launchd"
        );

        Ok(())
    }

    /// Unregister the service from launchd
    ///
    /// This unloads the service with launchctl and removes the plist file.
    pub fn unregister(&self) -> Result<()> {
        if !self.plist_path.exists() {
            return Err(Error::Daemon(format!(
                "Service {} is not registered (plist not found)",
                self.label
            )));
        }

        // Unload with launchctl
        let output = Command::new("launchctl")
            .args(["unload", "-w"])
            .arg(&self.plist_path)
            .output()
            .map_err(|e| Error::Daemon(format!("Failed to run launchctl unload: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Only warn, continue with removal
            tracing::warn!(
                error = stderr.trim(),
                "launchctl unload returned non-zero status"
            );
        }

        // Remove plist file
        fs::remove_file(&self.plist_path)
            .map_err(|e| Error::Daemon(format!("Failed to remove plist file: {}", e)))?;

        tracing::info!(
            label = %self.label,
            "Service unregistered from launchd"
        );

        Ok(())
    }

    /// Check if the service is registered
    pub fn is_registered(&self) -> bool {
        if !self.plist_path.exists() {
            return false;
        }

        // Check with launchctl list
        let output = Command::new("launchctl")
            .args(["list", &self.label])
            .output();

        match output {
            Ok(result) => result.status.success(),
            Err(_) => false,
        }
    }

    /// Check if the service is running
    pub fn is_running(&self) -> bool {
        let output = Command::new("launchctl")
            .args(["list", &self.label])
            .output();

        match output {
            Ok(result) => {
                if !result.status.success() {
                    return false;
                }
                // Parse output to check PID
                let stdout = String::from_utf8_lossy(&result.stdout);
                // launchctl list output format: "PID\tStatus\tLabel"
                // If PID is "-", the service is not running
                for line in stdout.lines() {
                    if line.contains(&self.label) {
                        let parts: Vec<&str> = line.split('\t').collect();
                        if let Some(pid_str) = parts.first() {
                            return *pid_str != "-" && pid_str.parse::<u32>().is_ok();
                        }
                    }
                }
                false
            }
            Err(_) => false,
        }
    }

    /// Get the status of the service
    pub fn status(&self) -> Result<LaunchdStatus> {
        let registered = self.is_registered();
        let running = if registered { self.is_running() } else { false };

        Ok(LaunchdStatus {
            registered,
            running,
            plist_path: self.plist_path.clone(),
            label: self.label.clone(),
        })
    }
}

impl Default for Launchd {
    fn default() -> Self {
        Self::new()
    }
}

/// Status of the launchd service
#[derive(Debug, Clone)]
pub struct LaunchdStatus {
    /// Whether the service is registered with launchd
    pub registered: bool,
    /// Whether the service is currently running
    pub running: bool,
    /// Path to the plist file
    pub plist_path: PathBuf,
    /// Service label
    pub label: String,
}

/// Escape special XML characters in a string
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_plist_path() {
        let plist_path = Launchd::default_plist_path();
        assert!(
            plist_path
                .to_string_lossy()
                .contains("Library/LaunchAgents")
        );
        assert!(
            plist_path
                .to_string_lossy()
                .ends_with("com.github.kawaz.authsock-filter.plist")
        );
    }

    #[test]
    fn test_generate_plist() {
        let temp_dir = TempDir::new().unwrap();
        let plist_path = temp_dir
            .path()
            .join("com.github.kawaz.authsock-filter.plist");
        let launchd = Launchd::with_plist_path(plist_path);

        let plist = launchd
            .generate_plist(&["--upstream".to_string(), "/tmp/agent.sock".to_string()])
            .unwrap();

        assert!(plist.contains("<key>Label</key>"));
        assert!(plist.contains("com.github.kawaz.authsock-filter"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<true/>"));
        assert!(plist.contains("--upstream"));
        assert!(plist.contains("/tmp/agent.sock"));
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("hello"), "hello");
        assert_eq!(escape_xml("<test>"), "&lt;test&gt;");
        assert_eq!(escape_xml("a & b"), "a &amp; b");
        assert_eq!(escape_xml("\"quoted\""), "&quot;quoted&quot;");
    }

    #[test]
    fn test_launchd_with_custom_path() {
        let custom_path = PathBuf::from("/tmp/test.plist");
        let launchd = Launchd::with_plist_path(custom_path.clone());
        assert_eq!(launchd.plist_path(), &custom_path);
    }
}
