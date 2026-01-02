//! Register command - register as an OS service

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::info;

use crate::cli::args::{RegisterArgs, SocketSpec};

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

/// Generate launchd plist content
fn generate_launchd_plist(
    name: &str,
    exe_path: &str,
    upstream: Option<&PathBuf>,
    socket_specs: &[SocketSpec],
) -> String {
    let mut args = vec![exe_path.to_string(), "run".to_string()];

    if let Some(up) = upstream {
        args.push("--upstream".to_string());
        args.push(up.display().to_string());
    }

    for spec in socket_specs {
        args.push("--socket".to_string());
        args.push(spec.path.display().to_string());
        for filter in &spec.filters {
            args.push("--filter".to_string());
            args.push(filter.clone());
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
fn generate_systemd_unit(
    _name: &str,
    exe_path: &str,
    upstream: Option<&PathBuf>,
    socket_specs: &[SocketSpec],
) -> String {
    let mut exec_start = format!("{} run", exe_path);

    if let Some(up) = upstream {
        exec_start.push_str(&format!(" --upstream {}", up.display()));
    }

    for spec in socket_specs {
        exec_start.push_str(&format!(" --socket {}", spec.path.display()));
        for filter in &spec.filters {
            exec_start.push_str(&format!(" --filter {}", filter));
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
pub async fn execute(args: RegisterArgs) -> Result<()> {
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
                bail!(
                    "Service is already registered: {}\nUse 'unregister' first to remove it.",
                    plist_path.display()
                );
            }

            // Generate and write plist
            let socket_specs = args.parse_socket_specs();
            let plist_content = generate_launchd_plist(
                &args.name,
                &exe_path,
                args.upstream.as_ref(),
                &socket_specs,
            );

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
                println!("  launchctl load -w {}", plist_path.display());
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
                bail!(
                    "Service is already registered: {}\nUse 'unregister' first to remove it.",
                    unit_path.display()
                );
            }

            // Generate and write unit file
            let socket_specs = args.parse_socket_specs();
            let unit_content =
                generate_systemd_unit(&args.name, &exe_path, args.upstream.as_ref(), &socket_specs);

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
                println!("  systemctl --user start {}", args.name);
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
