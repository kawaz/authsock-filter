//! Linux systemd integration
//!
//! Provides functionality to register authsock-filter as a systemd user service:
//! - Generate systemd unit file
//! - Register with systemctl --user enable
//! - Unregister with systemctl --user disable

use crate::error::{Error, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Service name for systemd
const SERVICE_NAME: &str = "authsock-filter.service";

/// Systemd manager for Linux
#[derive(Debug)]
pub struct Systemd {
    /// Path to the unit file
    unit_path: PathBuf,
    /// Service name
    service_name: String,
}

impl Systemd {
    /// Create a new Systemd manager with default unit file location
    ///
    /// Unit file location: ~/.config/systemd/user/authsock-filter.service
    pub fn new() -> Self {
        Self {
            unit_path: Self::default_unit_path(),
            service_name: SERVICE_NAME.to_string(),
        }
    }

    /// Create a new Systemd manager with a custom unit file path
    pub fn with_unit_path(unit_path: PathBuf) -> Self {
        Self {
            unit_path,
            service_name: SERVICE_NAME.to_string(),
        }
    }

    /// Get the default unit file path
    pub fn default_unit_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("~"))
                    .join(".config")
            })
            .join("systemd")
            .join("user")
            .join(SERVICE_NAME)
    }

    /// Get the unit file path
    pub fn unit_path(&self) -> &PathBuf {
        &self.unit_path
    }

    /// Get the service name
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Generate the systemd unit file content
    ///
    /// # Arguments
    /// * `args` - Additional arguments to pass to authsock-filter run command
    pub fn generate_unit(&self, args: &[String]) -> Result<String> {
        let executable = std::env::current_exe()
            .map_err(|e| Error::Daemon(format!("Failed to get current executable path: {}", e)))?;

        let executable_path = executable.to_string_lossy();

        // Build ExecStart command
        let mut exec_start_parts = vec![executable_path.to_string(), "run".to_string()];
        exec_start_parts.extend(args.iter().cloned());
        let exec_start = exec_start_parts.join(" ");

        let unit = format!(
            r#"[Unit]
Description=SSH Agent Filter Proxy
Documentation=https://github.com/kawaz/authsock-filter
After=default.target

[Service]
Type=simple
ExecStart={exec_start}
Restart=on-failure
RestartSec=5

# Environment
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
"#,
            exec_start = exec_start,
        );

        Ok(unit)
    }

    /// Register the service with systemd
    ///
    /// This generates the unit file and enables it with systemctl --user.
    pub fn register(&self, args: &[String]) -> Result<()> {
        // Check if already registered
        if self.is_registered() {
            return Err(Error::Daemon(format!(
                "Service {} is already registered",
                self.service_name
            )));
        }

        // Ensure systemd user directory exists
        if let Some(parent) = self.unit_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Error::Daemon(format!("Failed to create systemd user directory: {}", e))
            })?;
        }

        // Generate and write unit file
        let unit_content = self.generate_unit(args)?;
        fs::write(&self.unit_path, &unit_content)
            .map_err(|e| Error::Daemon(format!("Failed to write unit file: {}", e)))?;

        tracing::debug!(unit_path = %self.unit_path.display(), "Wrote unit file");

        // Reload systemd user daemon
        let reload_output = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output()
            .map_err(|e| Error::Daemon(format!("Failed to run systemctl daemon-reload: {}", e)))?;

        if !reload_output.status.success() {
            let stderr = String::from_utf8_lossy(&reload_output.stderr);
            tracing::warn!(error = stderr.trim(), "systemctl daemon-reload warning");
        }

        // Enable the service
        let enable_output = Command::new("systemctl")
            .args(["--user", "enable", &self.service_name])
            .output()
            .map_err(|e| Error::Daemon(format!("Failed to run systemctl enable: {}", e)))?;

        if !enable_output.status.success() {
            let stderr = String::from_utf8_lossy(&enable_output.stderr);
            // Clean up unit file on failure
            fs::remove_file(&self.unit_path).ok();
            return Err(Error::Daemon(format!(
                "systemctl enable failed: {}",
                stderr.trim()
            )));
        }

        // Start the service
        let start_output = Command::new("systemctl")
            .args(["--user", "start", &self.service_name])
            .output()
            .map_err(|e| Error::Daemon(format!("Failed to run systemctl start: {}", e)))?;

        if !start_output.status.success() {
            let stderr = String::from_utf8_lossy(&start_output.stderr);
            tracing::warn!(error = stderr.trim(), "systemctl start warning");
        }

        tracing::info!(
            service = %self.service_name,
            unit_path = %self.unit_path.display(),
            "Service registered with systemd"
        );

        Ok(())
    }

    /// Unregister the service from systemd
    ///
    /// This stops and disables the service with systemctl, then removes the unit file.
    pub fn unregister(&self) -> Result<()> {
        if !self.unit_path.exists() {
            return Err(Error::Daemon(format!(
                "Service {} is not registered (unit file not found)",
                self.service_name
            )));
        }

        // Stop the service first
        let stop_output = Command::new("systemctl")
            .args(["--user", "stop", &self.service_name])
            .output()
            .map_err(|e| Error::Daemon(format!("Failed to run systemctl stop: {}", e)))?;

        if !stop_output.status.success() {
            let stderr = String::from_utf8_lossy(&stop_output.stderr);
            tracing::warn!(error = stderr.trim(), "systemctl stop warning");
        }

        // Disable the service
        let disable_output = Command::new("systemctl")
            .args(["--user", "disable", &self.service_name])
            .output()
            .map_err(|e| Error::Daemon(format!("Failed to run systemctl disable: {}", e)))?;

        if !disable_output.status.success() {
            let stderr = String::from_utf8_lossy(&disable_output.stderr);
            tracing::warn!(error = stderr.trim(), "systemctl disable warning");
        }

        // Remove unit file
        fs::remove_file(&self.unit_path)
            .map_err(|e| Error::Daemon(format!("Failed to remove unit file: {}", e)))?;

        // Reload systemd daemon
        let reload_output = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();

        if let Err(e) = reload_output {
            tracing::warn!(error = %e, "Failed to reload systemd daemon");
        }

        tracing::info!(
            service = %self.service_name,
            "Service unregistered from systemd"
        );

        Ok(())
    }

    /// Check if the service is registered (unit file exists)
    pub fn is_registered(&self) -> bool {
        self.unit_path.exists()
    }

    /// Check if the service is enabled
    pub fn is_enabled(&self) -> bool {
        let output = Command::new("systemctl")
            .args(["--user", "is-enabled", &self.service_name])
            .output();

        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                stdout.trim() == "enabled"
            }
            Err(_) => false,
        }
    }

    /// Check if the service is running
    pub fn is_running(&self) -> bool {
        let output = Command::new("systemctl")
            .args(["--user", "is-active", &self.service_name])
            .output();

        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                stdout.trim() == "active"
            }
            Err(_) => false,
        }
    }

    /// Get the status of the service
    pub fn status(&self) -> Result<SystemdStatus> {
        let registered = self.is_registered();
        let enabled = if registered { self.is_enabled() } else { false };
        let running = if registered { self.is_running() } else { false };

        Ok(SystemdStatus {
            registered,
            enabled,
            running,
            unit_path: self.unit_path.clone(),
            service_name: self.service_name.clone(),
        })
    }

    /// Restart the service
    pub fn restart(&self) -> Result<()> {
        if !self.is_registered() {
            return Err(Error::Daemon(format!(
                "Service {} is not registered",
                self.service_name
            )));
        }

        let output = Command::new("systemctl")
            .args(["--user", "restart", &self.service_name])
            .output()
            .map_err(|e| Error::Daemon(format!("Failed to run systemctl restart: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Daemon(format!(
                "systemctl restart failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }
}

impl Default for Systemd {
    fn default() -> Self {
        Self::new()
    }
}

/// Status of the systemd service
#[derive(Debug, Clone)]
pub struct SystemdStatus {
    /// Whether the unit file exists
    pub registered: bool,
    /// Whether the service is enabled
    pub enabled: bool,
    /// Whether the service is currently running
    pub running: bool,
    /// Path to the unit file
    pub unit_path: PathBuf,
    /// Service name
    pub service_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_unit_path() {
        let unit_path = Systemd::default_unit_path();
        assert!(unit_path.to_string_lossy().contains("systemd/user"));
        assert!(
            unit_path
                .to_string_lossy()
                .ends_with("authsock-filter.service")
        );
    }

    #[test]
    fn test_generate_unit() {
        let temp_dir = TempDir::new().unwrap();
        let unit_path = temp_dir.path().join("authsock-filter.service");
        let systemd = Systemd::with_unit_path(unit_path);

        let unit = systemd
            .generate_unit(&["--upstream".to_string(), "/tmp/agent.sock".to_string()])
            .unwrap();

        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("[Install]"));
        assert!(unit.contains("Description=SSH Agent Filter Proxy"));
        assert!(unit.contains("Type=simple"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("--upstream"));
        assert!(unit.contains("/tmp/agent.sock"));
    }

    #[test]
    fn test_systemd_with_custom_path() {
        let custom_path = PathBuf::from("/tmp/test.service");
        let systemd = Systemd::with_unit_path(custom_path.clone());
        assert_eq!(systemd.unit_path(), &custom_path);
    }

    #[test]
    fn test_is_registered_false() {
        let temp_dir = TempDir::new().unwrap();
        let unit_path = temp_dir.path().join("nonexistent.service");
        let systemd = Systemd::with_unit_path(unit_path);

        assert!(!systemd.is_registered());
    }
}
