//! Service management commands - register/unregister

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;
use tracing::info;

use super::detect_version_manager;
use crate::cli::args::{RegisterArgs, UnregisterArgs, UpstreamGroup};

// ============================================================================
// Executable path resolution
// ============================================================================

/// Resolve the executable path for service registration
fn resolve_service_executable(
    explicit_path: Option<PathBuf>,
    allow_versioned: bool,
) -> Result<PathBuf> {
    // 1. If explicitly specified, validate and use it
    if let Some(path) = explicit_path {
        if !path.exists() {
            bail!(
                "Specified executable path does not exist: {}",
                path.display()
            );
        }
        return path.canonicalize().context("Failed to canonicalize path");
    }

    // 2. Check if argv[0] is a stable path (e.g., shim)
    if let Some(arg0) = std::env::args().next() {
        let arg0_path = PathBuf::from(&arg0);
        if arg0_path.is_absolute()
            && arg0_path.exists()
            && detect_version_manager(&arg0_path).is_none()
        {
            return Ok(arg0_path);
        }
    }

    // 3. Use current executable
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;

    // 4. Check if it's a version-managed path
    if let Some(info) = detect_version_manager(&current_exe) {
        if allow_versioned {
            eprintln!(
                "Warning: Registering with version-managed path.\nPath: {}\n",
                current_exe.display()
            );
        } else {
            bail!(
                "Executable is under {} version manager: {}\n\
                 Use --allow-versioned-path to proceed, or specify --executable with a stable path.",
                info.name,
                current_exe.display()
            );
        }
    }

    Ok(current_exe)
}

// ============================================================================
// macOS launchd support
// ============================================================================

#[cfg(target_os = "macos")]
mod launchd {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct LaunchdPlist {
        pub label: String,
        pub program_arguments: Vec<String>,
        pub run_at_load: bool,
        pub keep_alive: bool,
        pub standard_out_path: String,
        pub standard_error_path: String,
        pub environment_variables: HashMap<String, String>,
    }

    pub fn plist_path(name: &str) -> PathBuf {
        dirs::home_dir()
            .expect("Failed to get home directory")
            .join("Library/LaunchAgents")
            .join(format!("com.github.kawaz.{}.plist", name))
    }

    pub fn label(name: &str) -> String {
        format!("com.github.kawaz.{}", name)
    }

    pub fn generate_plist(
        name: &str,
        exe_path: &str,
        upstream_groups: &[UpstreamGroup],
    ) -> Result<Vec<u8>> {
        let mut args = vec![exe_path.to_string(), "run".to_string()];

        for group in upstream_groups {
            args.push("--upstream".to_string());
            args.push(group.path.display().to_string());
            for spec in &group.sockets {
                args.push("--socket".to_string());
                args.push(spec.path.display().to_string());
                for filter in &spec.filters {
                    args.push(filter.clone());
                }
            }
        }

        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );

        let plist = LaunchdPlist {
            label: label(name),
            program_arguments: args,
            run_at_load: true,
            keep_alive: true,
            standard_out_path: format!("/tmp/{}.stdout.log", name),
            standard_error_path: format!("/tmp/{}.stderr.log", name),
            environment_variables: env,
        };

        let mut buf = Vec::new();
        plist::to_writer_xml(&mut buf, &plist).context("Failed to serialize plist")?;
        Ok(buf)
    }
}

// ============================================================================
// Linux systemd support
// ============================================================================

#[cfg(target_os = "linux")]
mod systemd {
    use super::*;

    pub fn unit_path(name: &str) -> PathBuf {
        dirs::config_dir()
            .expect("Failed to get config directory")
            .join("systemd/user")
            .join(format!("{}.service", name))
    }

    pub fn generate_unit(_name: &str, exe_path: &str, upstream_groups: &[UpstreamGroup]) -> String {
        let mut exec_start = format!("{} run", exe_path);

        for group in upstream_groups {
            exec_start.push_str(&format!(" --upstream {}", group.path.display()));
            for spec in &group.sockets {
                exec_start.push_str(&format!(" --socket {}", spec.path.display()));
                for filter in &spec.filters {
                    exec_start.push_str(&format!(" {}", filter));
                }
            }
        }

        format!(
            r#"[Unit]
Description=SSH agent proxy with key filtering
After=default.target

[Service]
Type=simple
ExecStart={exec_start}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#
        )
    }
}

// ============================================================================
// Public API - macOS
// ============================================================================

#[cfg(target_os = "macos")]
pub async fn register(args: RegisterArgs) -> Result<()> {
    let exe_path = resolve_service_executable(args.executable.clone(), args.allow_versioned_path)?;
    let exe_path_str = exe_path.display().to_string();

    info!(name = %args.name, executable = %exe_path_str, "Registering launchd service");

    let plist_path = launchd::plist_path(&args.name);

    // Create LaunchAgents directory if needed
    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent).context("Failed to create LaunchAgents directory")?;
    }

    // Unload and remove existing service if present
    if plist_path.exists() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", plist_path.to_str().unwrap()])
            .status();
        fs::remove_file(&plist_path).context("Failed to remove existing plist")?;
        println!("Removed existing service");
    }

    // Generate and write plist
    let upstream_groups = args.parse_upstream_groups();
    if upstream_groups.is_empty() {
        bail!("No upstream/socket configuration provided. Use --upstream and --socket options.");
    }

    let plist_content = launchd::generate_plist(&args.name, &exe_path_str, &upstream_groups)?;
    fs::write(&plist_path, &plist_content).context("Failed to write launchd plist")?;

    println!("Created: {}", plist_path.display());

    // Load and start the service
    let status = std::process::Command::new("launchctl")
        .args(["load", "-w", plist_path.to_str().unwrap()])
        .status()
        .context("Failed to run launchctl")?;

    if !status.success() {
        bail!("Failed to load service with launchctl");
    }

    println!("Service registered and started successfully!");
    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn unregister(args: UnregisterArgs) -> Result<()> {
    info!(name = %args.name, "Unregistering launchd service");

    let plist_path = launchd::plist_path(&args.name);

    if !plist_path.exists() {
        println!("Service is not registered");
        return Ok(());
    }

    // Unload the service
    let _ = std::process::Command::new("launchctl")
        .args(["unload", "-w", plist_path.to_str().unwrap()])
        .status();

    // Remove the plist file
    fs::remove_file(&plist_path).context("Failed to remove launchd plist")?;

    println!("Service unregistered successfully!");
    Ok(())
}

// ============================================================================
// Public API - Linux
// ============================================================================

#[cfg(target_os = "linux")]
pub async fn register(args: RegisterArgs) -> Result<()> {
    let exe_path = resolve_service_executable(args.executable.clone(), args.allow_versioned_path)?;
    let exe_path_str = exe_path.display().to_string();

    info!(name = %args.name, executable = %exe_path_str, "Registering systemd service");

    let unit_path = systemd::unit_path(&args.name);

    // Create systemd user directory if needed
    if let Some(parent) = unit_path.parent() {
        fs::create_dir_all(parent).context("Failed to create systemd user directory")?;
    }

    // Stop and remove existing service if present
    if unit_path.exists() {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", &args.name])
            .status();
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", &args.name])
            .status();
        fs::remove_file(&unit_path).context("Failed to remove existing unit file")?;
        println!("Removed existing service");
    }

    // Generate and write unit file
    let upstream_groups = args.parse_upstream_groups();
    if upstream_groups.is_empty() {
        bail!("No upstream/socket configuration provided. Use --upstream and --socket options.");
    }

    let unit_content = systemd::generate_unit(&args.name, &exe_path_str, &upstream_groups);
    fs::write(&unit_path, &unit_content).context("Failed to write systemd unit file")?;

    println!("Created: {}", unit_path.display());

    // Reload, enable and start
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "enable", &args.name])
        .status();

    let status = std::process::Command::new("systemctl")
        .args(["--user", "start", &args.name])
        .status()
        .context("Failed to start service")?;

    if !status.success() {
        bail!("Failed to start service");
    }

    println!("Service registered and started successfully!");
    Ok(())
}

#[cfg(target_os = "linux")]
pub async fn unregister(args: UnregisterArgs) -> Result<()> {
    info!(name = %args.name, "Unregistering systemd service");

    let unit_path = systemd::unit_path(&args.name);

    if !unit_path.exists() {
        println!("Service is not registered");
        return Ok(());
    }

    // Stop and disable
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "stop", &args.name])
        .status();
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", &args.name])
        .status();

    // Remove the unit file
    fs::remove_file(&unit_path).context("Failed to remove systemd unit file")?;

    // Reload systemd
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    println!("Service unregistered successfully!");
    Ok(())
}

// ============================================================================
// Public API - Unsupported platforms
// ============================================================================

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn register(_args: RegisterArgs) -> Result<()> {
    bail!("Service registration is not supported on this platform")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn unregister(_args: UnregisterArgs) -> Result<()> {
    bail!("Service management is not supported on this platform")
}
