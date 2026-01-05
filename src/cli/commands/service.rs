//! Service management commands - register/unregister/start/stop/status

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;
use tracing::info;

use crate::cli::args::{RegisterArgs, UnregisterArgs, UpstreamGroup};

/// Service manager type
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum ServiceManager {
    /// macOS launchd
    Launchd,
    /// Linux systemd (user)
    SystemdUser,
    /// Linux systemd (system)
    SystemdSystem,
}

/// Detect the available service manager
fn detect_service_manager() -> Result<ServiceManager> {
    #[cfg(target_os = "macos")]
    {
        Ok(ServiceManager::Launchd)
    }
    #[cfg(target_os = "linux")]
    {
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

/// Default service name
const DEFAULT_SERVICE_NAME: &str = "authsock-filter";

/// Get launchd plist path
fn launchd_plist_path(name: &str) -> PathBuf {
    dirs::home_dir()
        .expect("Failed to get home directory")
        .join("Library/LaunchAgents")
        .join(format!("com.github.kawaz.{}.plist", name))
}

/// Get launchd service label
fn launchd_label(name: &str) -> String {
    format!("com.github.kawaz.{}", name)
}

/// Get systemd unit path
fn systemd_unit_path(name: &str) -> PathBuf {
    dirs::config_dir()
        .expect("Failed to get config directory")
        .join("systemd/user")
        .join(format!("{}.service", name))
}

/// Generate launchd plist content
fn generate_launchd_plist(name: &str, exe_path: &str, upstream_groups: &[UpstreamGroup]) -> String {
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

    let args_xml: String = args
        .iter()
        .map(|a| format!("    <string>{}</string>", a))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.github.kawaz.{name}</string>

  <key>ProgramArguments</key>
  <array>
{args_xml}
  </array>

  <key>RunAtLoad</key>
  <true/>

  <key>KeepAlive</key>
  <true/>

  <key>StandardOutPath</key>
  <string>/tmp/{name}.stdout.log</string>

  <key>StandardErrorPath</key>
  <string>/tmp/{name}.stderr.log</string>

  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>/usr/local/bin:/usr/bin:/bin</string>
  </dict>
</dict>
</plist>
"#,
        name = name,
        args_xml = args_xml
    )
}

/// Generate systemd unit content
fn generate_systemd_unit(_name: &str, exe_path: &str, upstream_groups: &[UpstreamGroup]) -> String {
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
Description=SSH agent proxy with filtering and logging
After=default.target

[Service]
Type=simple
ExecStart={exec_start}
Restart=on-failure
RestartSec=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=true

[Install]
WantedBy=default.target
"#,
        exec_start = exec_start
    )
}

/// Execute the register command
pub async fn register(args: RegisterArgs) -> Result<()> {
    let service_manager = detect_service_manager()?;
    let exe_path = std::env::current_exe()
        .context("Failed to get current executable path")?
        .display()
        .to_string();

    info!(
        service_manager = ?service_manager,
        name = %args.name,
        "Registering service"
    );

    match service_manager {
        ServiceManager::Launchd => {
            let plist_path = launchd_plist_path(&args.name);

            // Create LaunchAgents directory if needed
            if let Some(parent) = plist_path.parent() {
                fs::create_dir_all(parent).context("Failed to create LaunchAgents directory")?;
            }

            // Check if already registered
            if plist_path.exists() {
                if args.force {
                    // Unload existing service first
                    let _ = std::process::Command::new("launchctl")
                        .args(["unload", plist_path.to_str().unwrap()])
                        .status();
                    fs::remove_file(&plist_path).context("Failed to remove existing plist")?;
                    println!("Removed existing registration: {}", plist_path.display());
                } else {
                    bail!(
                        "Service is already registered: {}\nUse 'unregister' first to remove it, or use '--force' to re-register.",
                        plist_path.display()
                    );
                }
            }

            // Generate and write plist
            let upstream_groups = args.parse_upstream_groups();
            let plist_content = generate_launchd_plist(&args.name, &exe_path, &upstream_groups);

            fs::write(&plist_path, &plist_content).context("Failed to write launchd plist")?;

            println!("Created launchd plist: {}", plist_path.display());

            // Load the service if requested
            if args.start {
                let status = std::process::Command::new("launchctl")
                    .args(["load", "-w", plist_path.to_str().unwrap()])
                    .status()
                    .context("Failed to run launchctl")?;

                if !status.success() {
                    bail!("Failed to load service with launchctl");
                }

                println!("Service started successfully");
            } else {
                println!();
                println!("To start the service, run:");
                println!("  authsock-filter service start");
            }
        }

        ServiceManager::SystemdUser => {
            let unit_path = systemd_unit_path(&args.name);

            // Create systemd user directory if needed
            if let Some(parent) = unit_path.parent() {
                fs::create_dir_all(parent).context("Failed to create systemd user directory")?;
            }

            // Check if already registered
            if unit_path.exists() {
                if args.force {
                    // Stop and disable existing service first
                    let _ = std::process::Command::new("systemctl")
                        .args(["--user", "stop", &args.name])
                        .status();
                    let _ = std::process::Command::new("systemctl")
                        .args(["--user", "disable", &args.name])
                        .status();
                    fs::remove_file(&unit_path).context("Failed to remove existing unit file")?;
                    println!("Removed existing registration: {}", unit_path.display());
                } else {
                    bail!(
                        "Service is already registered: {}\nUse 'unregister' first to remove it, or use '--force' to re-register.",
                        unit_path.display()
                    );
                }
            }

            // Generate and write unit file
            let upstream_groups = args.parse_upstream_groups();
            let unit_content = generate_systemd_unit(&args.name, &exe_path, &upstream_groups);

            fs::write(&unit_path, &unit_content).context("Failed to write systemd unit file")?;

            println!("Created systemd unit: {}", unit_path.display());

            // Reload systemd
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();

            // Enable if requested
            if args.enable {
                let status = std::process::Command::new("systemctl")
                    .args(["--user", "enable", &args.name])
                    .status()
                    .context("Failed to run systemctl enable")?;

                if !status.success() {
                    eprintln!("Warning: Failed to enable service");
                } else {
                    println!("Service enabled");
                }
            }

            // Start if requested
            if args.start {
                let status = std::process::Command::new("systemctl")
                    .args(["--user", "start", &args.name])
                    .status()
                    .context("Failed to run systemctl start")?;

                if !status.success() {
                    bail!("Failed to start service");
                }

                println!("Service started successfully");
            } else {
                println!();
                println!("To start the service, run:");
                println!("  authsock-filter service start");
            }
        }

        ServiceManager::SystemdSystem => {
            bail!("System-wide systemd installation not yet supported. Use user mode.");
        }
    }

    println!();
    println!("Service registered successfully!");

    Ok(())
}

/// Execute the unregister command
pub async fn unregister(args: UnregisterArgs) -> Result<()> {
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

        ServiceManager::SystemdUser | ServiceManager::SystemdSystem => {
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

    println!("Configuration files purged.");

    Ok(())
}

/// Start the registered service
pub async fn start() -> Result<()> {
    let service_manager = detect_service_manager()?;
    let name = DEFAULT_SERVICE_NAME;

    match service_manager {
        ServiceManager::Launchd => {
            let plist_path = launchd_plist_path(name);

            if !plist_path.exists() {
                bail!("Service is not registered. Run 'authsock-filter service register' first.");
            }

            let status = std::process::Command::new("launchctl")
                .args(["load", "-w", plist_path.to_str().unwrap()])
                .status()
                .context("Failed to run launchctl")?;

            if !status.success() {
                bail!("Failed to start service");
            }

            println!("Service started successfully");
        }

        ServiceManager::SystemdUser | ServiceManager::SystemdSystem => {
            let unit_path = systemd_unit_path(name);

            if !unit_path.exists() {
                bail!("Service is not registered. Run 'authsock-filter service register' first.");
            }

            let status = std::process::Command::new("systemctl")
                .args(["--user", "start", name])
                .status()
                .context("Failed to run systemctl")?;

            if !status.success() {
                bail!("Failed to start service");
            }

            println!("Service started successfully");
        }
    }

    Ok(())
}

/// Stop the registered service
pub async fn stop() -> Result<()> {
    let service_manager = detect_service_manager()?;
    let name = DEFAULT_SERVICE_NAME;

    match service_manager {
        ServiceManager::Launchd => {
            let plist_path = launchd_plist_path(name);

            if !plist_path.exists() {
                bail!("Service is not registered.");
            }

            let status = std::process::Command::new("launchctl")
                .args(["unload", plist_path.to_str().unwrap()])
                .status()
                .context("Failed to run launchctl")?;

            if !status.success() {
                bail!("Failed to stop service");
            }

            println!("Service stopped successfully");
        }

        ServiceManager::SystemdUser | ServiceManager::SystemdSystem => {
            let status = std::process::Command::new("systemctl")
                .args(["--user", "stop", name])
                .status()
                .context("Failed to run systemctl")?;

            if !status.success() {
                bail!("Failed to stop service");
            }

            println!("Service stopped successfully");
        }
    }

    Ok(())
}

/// Show service status
pub async fn status() -> Result<()> {
    let service_manager = detect_service_manager()?;
    let name = DEFAULT_SERVICE_NAME;

    match service_manager {
        ServiceManager::Launchd => {
            let plist_path = launchd_plist_path(name);
            let label = launchd_label(name);

            println!("Service: {}", label);
            println!("Plist: {}", plist_path.display());

            if !plist_path.exists() {
                println!("Status: Not registered");
                return Ok(());
            }

            // Check if service is running
            let output = std::process::Command::new("launchctl")
                .args(["list", &label])
                .output()
                .context("Failed to run launchctl")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse PID from output (format: "PID\tStatus\tLabel")
                if let Some(line) = stdout.lines().next() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 2 {
                        let pid = parts[0];
                        let exit_code = parts[1];
                        if pid != "-" {
                            println!("Status: Running (PID: {})", pid);
                        } else {
                            println!("Status: Stopped (last exit code: {})", exit_code);
                        }
                    }
                }
            } else {
                println!("Status: Registered but not loaded");
            }
        }

        ServiceManager::SystemdUser | ServiceManager::SystemdSystem => {
            let unit_path = systemd_unit_path(name);

            println!("Service: {}", name);
            println!("Unit: {}", unit_path.display());

            if !unit_path.exists() {
                println!("Status: Not registered");
                return Ok(());
            }

            // Use systemctl status
            let status = std::process::Command::new("systemctl")
                .args(["--user", "status", name, "--no-pager"])
                .status()
                .context("Failed to run systemctl")?;

            if !status.success() {
                // Non-zero exit could mean inactive, not an error
            }
        }
    }

    Ok(())
}

/// Enable auto-start at login/boot
pub async fn enable() -> Result<()> {
    let service_manager = detect_service_manager()?;
    let name = DEFAULT_SERVICE_NAME;

    match service_manager {
        ServiceManager::Launchd => {
            let plist_path = launchd_plist_path(name);

            if !plist_path.exists() {
                bail!("Service is not registered. Run 'authsock-filter service register' first.");
            }

            // launchctl load -w enables the service
            let status = std::process::Command::new("launchctl")
                .args(["load", "-w", plist_path.to_str().unwrap()])
                .status()
                .context("Failed to run launchctl")?;

            if !status.success() {
                bail!("Failed to enable service");
            }

            println!("Service enabled (will start at login)");
        }

        ServiceManager::SystemdUser | ServiceManager::SystemdSystem => {
            let unit_path = systemd_unit_path(name);

            if !unit_path.exists() {
                bail!("Service is not registered. Run 'authsock-filter service register' first.");
            }

            let status = std::process::Command::new("systemctl")
                .args(["--user", "enable", name])
                .status()
                .context("Failed to run systemctl")?;

            if !status.success() {
                bail!("Failed to enable service");
            }

            println!("Service enabled (will start at login)");
        }
    }

    Ok(())
}

/// Disable auto-start at login/boot
pub async fn disable() -> Result<()> {
    let service_manager = detect_service_manager()?;
    let name = DEFAULT_SERVICE_NAME;

    match service_manager {
        ServiceManager::Launchd => {
            let plist_path = launchd_plist_path(name);

            if !plist_path.exists() {
                bail!("Service is not registered.");
            }

            // launchctl unload -w disables the service (but keeps it registered)
            let status = std::process::Command::new("launchctl")
                .args(["unload", "-w", plist_path.to_str().unwrap()])
                .status()
                .context("Failed to run launchctl")?;

            if !status.success() {
                bail!("Failed to disable service");
            }

            println!("Service disabled (will not start at login)");
        }

        ServiceManager::SystemdUser | ServiceManager::SystemdSystem => {
            let status = std::process::Command::new("systemctl")
                .args(["--user", "disable", name])
                .status()
                .context("Failed to run systemctl")?;

            if !status.success() {
                bail!("Failed to disable service");
            }

            println!("Service disabled (will not start at login)");
        }
    }

    Ok(())
}
