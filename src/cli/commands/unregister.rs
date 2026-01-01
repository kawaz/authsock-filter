//! Unregister command - unregister the OS service

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::info;

use crate::cli::args::UnregisterArgs;

/// Service manager type
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum ServiceManager {
    /// macOS launchd
    Launchd,
    /// Linux systemd (user)
    SystemdUser,
}

/// Detect the available service manager
fn detect_service_manager() -> Result<ServiceManager> {
    #[cfg(target_os = "macos")]
    {
        Ok(ServiceManager::Launchd)
    }
    #[cfg(target_os = "linux")]
    {
        // Check if systemd is available
        if PathBuf::from("/run/systemd/system").exists() {
            Ok(ServiceManager::SystemdUser)
        } else {
            bail!("No supported service manager found. systemd is required on Linux.");
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        bail!("Service registration is not supported on this platform");
    }
}

/// Get launchd plist path
fn launchd_plist_path(name: &str) -> PathBuf {
    dirs::home_dir()
        .expect("Failed to get home directory")
        .join("Library/LaunchAgents")
        .join(format!("com.github.kawaz.{}.plist", name))
}

/// Get systemd unit path
fn systemd_unit_path(name: &str) -> PathBuf {
    dirs::config_dir()
        .expect("Failed to get config directory")
        .join("systemd/user")
        .join(format!("{}.service", name))
}

/// Execute the unregister command
pub async fn execute(args: UnregisterArgs) -> Result<()> {
    let service_manager = detect_service_manager()?;

    info!(
        service_manager = ?service_manager,
        name = %args.name,
        purge = args.purge,
        "Unregistering service"
    );

    match service_manager {
        ServiceManager::Launchd => {
            let plist_path = launchd_plist_path(&args.name);

            if !plist_path.exists() {
                println!("Service is not registered: {}", plist_path.display());
                return Ok(());
            }

            // Unload the service first
            println!("Unloading service...");
            let status = std::process::Command::new("launchctl")
                .args(["unload", "-w", plist_path.to_str().unwrap()])
                .status()
                .context("Failed to run launchctl")?;

            if !status.success() {
                eprintln!("Warning: Failed to unload service (it may not be running)");
            }

            // Remove the plist file
            fs::remove_file(&plist_path).context("Failed to remove launchd plist")?;

            println!("Removed launchd plist: {}", plist_path.display());

            // Optionally remove configuration files
            if args.purge {
                purge_config_files()?;
            }
        }

        ServiceManager::SystemdUser => {
            let unit_path = systemd_unit_path(&args.name);

            if !unit_path.exists() {
                println!("Service is not registered: {}", unit_path.display());
                return Ok(());
            }

            // Stop the service if running
            println!("Stopping service...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "stop", &args.name])
                .status();

            // Disable the service
            println!("Disabling service...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "disable", &args.name])
                .status();

            // Remove the unit file
            fs::remove_file(&unit_path).context("Failed to remove systemd unit file")?;

            println!("Removed systemd unit: {}", unit_path.display());

            // Reload systemd
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();

            // Optionally remove configuration files
            if args.purge {
                purge_config_files()?;
            }
        }
    }

    println!();
    println!("Service unregistered successfully!");

    Ok(())
}

/// Remove configuration files
fn purge_config_files() -> Result<()> {
    println!();
    println!("Purging configuration files...");

    let config_paths = [
        dirs::config_dir().map(|p| p.join("authsock-filter")),
        dirs::home_dir().map(|p| p.join(".authsock-filter.toml")),
        dirs::home_dir().map(|p| p.join(".config/authsock-filter")),
    ];

    for path in config_paths.iter().flatten() {
        if path.exists() {
            if path.is_dir() {
                fs::remove_dir_all(path)
                    .with_context(|| format!("Failed to remove directory: {}", path.display()))?;
                println!("  Removed directory: {}", path.display());
            } else {
                fs::remove_file(path)
                    .with_context(|| format!("Failed to remove file: {}", path.display()))?;
                println!("  Removed file: {}", path.display());
            }
        }
    }

    // Remove runtime files
    let runtime_paths = [
        dirs::runtime_dir().map(|p| p.join("authsock-filter.pid")),
        Some(PathBuf::from("/tmp/authsock-filter.pid")),
    ];

    for path in runtime_paths.iter().flatten() {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("Failed to remove file: {}", path.display()))?;
            println!("  Removed file: {}", path.display());
        }
    }

    println!("Configuration files purged.");

    Ok(())
}
