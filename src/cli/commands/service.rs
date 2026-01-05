//! Service management commands - register/unregister/start/stop/status

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;
use tracing::info;

use super::detect_version_manager;
use crate::cli::args::{RegisterArgs, UnregisterArgs, UpstreamGroup};

// ============================================================================
// Common definitions
// ============================================================================

/// Default service name
const DEFAULT_SERVICE_NAME: &str = "authsock-filter";

// ============================================================================
// Executable path resolution
// ============================================================================

/// Resolve the executable path for service registration
///
/// If `explicit_path` is provided, validates and returns it.
/// Otherwise, uses `current_exe()` and errors if it's a version-managed path
/// (unless `allow_versioned` is true).
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
    //    When executed via mise shim, argv[0] is the shim path but current_exe() returns
    //    the version-specific binary path. If argv[0] is a stable path, use it.
    if let Some(arg0) = std::env::args().next() {
        let arg0_path = PathBuf::from(&arg0);
        if arg0_path.is_absolute()
            && arg0_path.exists()
            && detect_version_manager(&arg0_path).is_none()
        {
            // argv[0] is an absolute path that exists and is NOT under version manager
            return Ok(arg0_path);
        }
    }

    // 3. Use current executable
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;

    // 4. Check if it's a version-managed path
    if let Some(info) = detect_version_manager(&current_exe) {
        if allow_versioned {
            eprintln!(
                "Warning: Registering with version-managed path (--allow-versioned-path).\n\
                 Path: {}\n",
                current_exe.display()
            );
        } else {
            // Build error message with suggestions
            let mut msg = format!(
                "Warning: Executable path is under {} version manager.\n\
                 The service may stop working after version upgrade.\n\
                 Current path: {}\n",
                info.name,
                info.current_path.display()
            );

            // Get current command args for suggestions
            let args: Vec<String> = std::env::args().collect();
            let cmd_args: Vec<&str> = args
                .iter()
                .skip(1)
                .filter(|a| !a.starts_with("--executable") && !a.starts_with("--allow"))
                .map(|s| s.as_str())
                .collect();

            // Suggest stable shim paths if available
            if !info.suggestions.is_empty() {
                msg.push_str("\n# Re-run using a stable path (recommended):\n");
                for (shim_path, is_same) in &info.suggestions {
                    msg.push_str(&format!("{} {}\n", shim_path.display(), cmd_args.join(" ")));
                    if *is_same {
                        msg.push_str("(verified: same binary)\n");
                    }
                }
            }

            // Always show the --allow-versioned-path option with full command
            msg.push_str("\n# Or proceed with current version-specific path:\n");
            msg.push_str(&format!(
                "{} --allow-versioned-path {}\n",
                current_exe.display(),
                cmd_args.join(" ")
            ));

            bail!("{}", msg);
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

    /// launchd plist structure
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

    /// Get launchd plist path
    pub fn plist_path(name: &str) -> PathBuf {
        dirs::home_dir()
            .expect("Failed to get home directory")
            .join("Library/LaunchAgents")
            .join(format!("com.github.kawaz.{}.plist", name))
    }

    /// Get launchd service label
    pub fn label(name: &str) -> String {
        format!("com.github.kawaz.{}", name)
    }

    /// Generate launchd plist content
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

    /// Read and parse launchd plist file
    pub fn read_plist(path: &PathBuf) -> Result<LaunchdPlist> {
        plist::from_file(path).context("Failed to parse plist file")
    }

    /// Show detailed launchd service status
    pub fn show_status(name: &str) -> Result<()> {
        let path = plist_path(name);
        let lbl = label(name);

        println!("Service: {}", lbl);
        println!("Plist:   {}", path.display());

        if !path.exists() {
            println!("Status:  Not registered");
            return Ok(());
        }

        // Read plist file for configuration
        let plist_config = read_plist(&path).ok();

        // Get runtime info from launchctl print
        let uid = unsafe { libc::getuid() };
        let domain_target = format!("gui/{}/{}", uid, lbl);

        let output = std::process::Command::new("launchctl")
            .args(["print", &domain_target])
            .output()
            .context("Failed to run launchctl print")?;

        if !output.status.success() {
            // Service is registered but not loaded
            println!("Status:  Registered but not loaded");

            // Still show configuration from plist file
            if let Some(config) = &plist_config {
                show_plist_config(config);
            }

            println!();
            println!("To start the service:");
            println!("  authsock-filter service start");
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let runtime_info = parse_launchctl_print(&stdout);

        // Running state (from launchctl)
        let state = runtime_info
            .get("state")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let pid = runtime_info.get("pid").map(|s| s.as_str());
        let last_exit = runtime_info
            .get("last exit code")
            .map(|s| s.as_str())
            .unwrap_or("unknown");

        match state {
            "running" => {
                if let Some(p) = pid {
                    println!("Status:  Running (PID: {})", p);
                } else {
                    println!("Status:  Running");
                }
            }
            "waiting" => {
                println!("Status:  Waiting (last exit: {})", last_exit);
            }
            _ => {
                println!("Status:  {} (last exit: {})", state, last_exit);
            }
        }

        // Configuration (from plist file)
        if let Some(config) = &plist_config {
            show_plist_config(config);
        }

        // Run statistics (from launchctl)
        if let Some(runs) = runtime_info.get("runs") {
            println!();
            println!("Statistics:");
            println!("  Runs: {}", runs);
        }

        Ok(())
    }

    /// Display plist configuration
    fn show_plist_config(config: &LaunchdPlist) {
        println!();
        println!("Configuration:");
        println!(
            "  RunAtLoad:  {} (auto-start at login)",
            if config.run_at_load { "Yes" } else { "No" }
        );
        println!(
            "  KeepAlive:  {} (auto-restart on exit)",
            if config.keep_alive { "Yes" } else { "No" }
        );

        // Program arguments
        if !config.program_arguments.is_empty() {
            println!();
            println!("Command:");
            println!("  {}", config.program_arguments.join(" \\\n    "));
        }

        // Log paths
        println!();
        println!("Logs:");
        println!("  stdout: {}", config.standard_out_path);
        if config.standard_error_path != config.standard_out_path {
            println!("  stderr: {}", config.standard_error_path);
        }
    }

    /// Parse launchctl print output into key-value pairs
    fn parse_launchctl_print(output: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let mut current_key: Option<String> = None;
        let mut current_value = String::new();
        let mut brace_depth = 0;
        let mut first_line = true;

        for line in output.lines() {
            let trimmed = line.trim();

            // Skip the first line (service identifier like "gui/501/... = {")
            if first_line {
                first_line = false;
                continue;
            }

            // Track brace depth for multi-line values
            let open_braces = trimmed.matches('{').count();
            let close_braces = trimmed.matches('}').count();

            if brace_depth > 0 {
                // We're inside a multi-line value
                current_value.push('\n');
                current_value.push_str(trimmed);
                brace_depth += open_braces;
                brace_depth -= close_braces;

                if brace_depth == 0 {
                    if let Some(key) = current_key.take() {
                        map.insert(key, current_value.clone());
                    }
                    current_value.clear();
                }
                continue;
            }

            // Parse key = value lines
            if let Some(eq_pos) = trimmed.find(" = ") {
                let key = trimmed[..eq_pos].trim().to_string();
                let value = trimmed[eq_pos + 3..].trim().to_string();

                if value == "{" || value.ends_with(" {") {
                    // Start of multi-line value
                    current_key = Some(key);
                    current_value = value;
                    brace_depth = open_braces - close_braces;
                } else {
                    map.insert(key, value);
                }
            }
        }

        map
    }
}

// ============================================================================
// Linux systemd support
// ============================================================================

#[cfg(target_os = "linux")]
mod systemd {
    use super::*;

    /// Get systemd unit path
    pub fn unit_path(name: &str) -> PathBuf {
        dirs::config_dir()
            .expect("Failed to get config directory")
            .join("systemd/user")
            .join(format!("{}.service", name))
    }

    /// Generate systemd unit content
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

    /// Show detailed systemd service status
    pub fn show_status(name: &str) -> Result<()> {
        let path = unit_path(name);

        println!("Service: {}", name);
        println!("Unit:    {}", path.display());

        if !path.exists() {
            println!("Status:  Not registered");
            return Ok(());
        }

        // Get service properties
        let output = std::process::Command::new("systemctl")
            .args([
                "--user",
                "show",
                name,
                "--property=ActiveState,SubState,MainPID,ExecStart,Restart,UnitFileState",
            ])
            .output()
            .context("Failed to run systemctl show")?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut props: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();

            for line in stdout.lines() {
                if let Some(eq_pos) = line.find('=') {
                    let key = &line[..eq_pos];
                    let value = &line[eq_pos + 1..];
                    props.insert(key, value);
                }
            }

            // Status
            let active_state = props.get("ActiveState").copied().unwrap_or("unknown");
            let sub_state = props.get("SubState").copied().unwrap_or("");
            let pid = props.get("MainPID").copied().unwrap_or("0");

            match active_state {
                "active" => {
                    if pid != "0" {
                        println!("Status:  Running (PID: {}, {})", pid, sub_state);
                    } else {
                        println!("Status:  Active ({})", sub_state);
                    }
                }
                "inactive" => {
                    println!("Status:  Stopped");
                }
                "failed" => {
                    println!("Status:  Failed");
                }
                _ => {
                    println!("Status:  {} ({})", active_state, sub_state);
                }
            }

            // Configuration
            let unit_file_state = props.get("UnitFileState").copied().unwrap_or("unknown");
            let restart = props.get("Restart").copied().unwrap_or("no");

            println!();
            println!("Configuration:");
            println!(
                "  Enabled:    {} (auto-start at login)",
                if unit_file_state == "enabled" {
                    "Yes"
                } else {
                    "No"
                }
            );
            println!("  Restart:    {} (auto-restart policy)", restart);

            // Show ExecStart
            if let Some(exec_start) = props.get("ExecStart")
                && !exec_start.is_empty()
            {
                println!();
                println!("Command:");
                // ExecStart format: { path=...; argv[]=...; ... }
                // Extract just the path for display
                if let Some(path_start) = exec_start.find("path=") {
                    let after_path = &exec_start[path_start + 5..];
                    if let Some(end) = after_path.find(';') {
                        println!("  {}", &after_path[..end]);
                    }
                }
            }
        } else {
            // Fallback to simple status
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "status", name, "--no-pager"])
                .status();
        }

        Ok(())
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
    let plist_content = launchd::generate_plist(&args.name, &exe_path_str, &upstream_groups)?;

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

    println!();
    println!("Service registered successfully!");

    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn unregister(args: UnregisterArgs) -> Result<()> {
    info!(name = %args.name, purge = args.purge, "Unregistering launchd service");

    let plist_path = launchd::plist_path(&args.name);

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

    println!();
    println!("Service unregistered successfully!");

    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn start() -> Result<()> {
    let name = DEFAULT_SERVICE_NAME;
    let plist_path = launchd::plist_path(name);

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
    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn stop() -> Result<()> {
    let name = DEFAULT_SERVICE_NAME;
    let plist_path = launchd::plist_path(name);

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
    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn status() -> Result<()> {
    launchd::show_status(DEFAULT_SERVICE_NAME)
}

#[cfg(target_os = "macos")]
pub async fn enable() -> Result<()> {
    let name = DEFAULT_SERVICE_NAME;
    let plist_path = launchd::plist_path(name);

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
    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn disable() -> Result<()> {
    let name = DEFAULT_SERVICE_NAME;
    let plist_path = launchd::plist_path(name);

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
    let unit_content = systemd::generate_unit(&args.name, &exe_path_str, &upstream_groups);

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

    println!();
    println!("Service registered successfully!");

    Ok(())
}

#[cfg(target_os = "linux")]
pub async fn unregister(args: UnregisterArgs) -> Result<()> {
    info!(name = %args.name, purge = args.purge, "Unregistering systemd service");

    let unit_path = systemd::unit_path(&args.name);

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

    println!();
    println!("Service unregistered successfully!");

    Ok(())
}

#[cfg(target_os = "linux")]
pub async fn start() -> Result<()> {
    let name = DEFAULT_SERVICE_NAME;
    let unit_path = systemd::unit_path(name);

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
    Ok(())
}

#[cfg(target_os = "linux")]
pub async fn stop() -> Result<()> {
    let name = DEFAULT_SERVICE_NAME;

    let status = std::process::Command::new("systemctl")
        .args(["--user", "stop", name])
        .status()
        .context("Failed to run systemctl")?;

    if !status.success() {
        bail!("Failed to stop service");
    }

    println!("Service stopped successfully");
    Ok(())
}

#[cfg(target_os = "linux")]
pub async fn status() -> Result<()> {
    systemd::show_status(DEFAULT_SERVICE_NAME)
}

#[cfg(target_os = "linux")]
pub async fn enable() -> Result<()> {
    let name = DEFAULT_SERVICE_NAME;
    let unit_path = systemd::unit_path(name);

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
    Ok(())
}

#[cfg(target_os = "linux")]
pub async fn disable() -> Result<()> {
    let name = DEFAULT_SERVICE_NAME;

    let status = std::process::Command::new("systemctl")
        .args(["--user", "disable", name])
        .status()
        .context("Failed to run systemctl")?;

    if !status.success() {
        bail!("Failed to disable service");
    }

    println!("Service disabled (will not start at login)");
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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn start() -> Result<()> {
    bail!("Service management is not supported on this platform")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn stop() -> Result<()> {
    bail!("Service management is not supported on this platform")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn status() -> Result<()> {
    bail!("Service management is not supported on this platform")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn enable() -> Result<()> {
    bail!("Service management is not supported on this platform")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn disable() -> Result<()> {
    bail!("Service management is not supported on this platform")
}

// ============================================================================
// Common utilities
// ============================================================================

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
