//! Service management commands - register/unregister/reload

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

use super::detect_version_manager;
use crate::cli::args::{RegisterArgs, UnregisterArgs};
use crate::config::{find_config_file, load_config};

// ============================================================================
// Executable path resolution
// ============================================================================

/// Find executable candidates in PATH and known shim locations
fn find_executable_candidates(name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Check PATH
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        let candidate = PathBuf::from(dir).join(name);
        if let Some(path) = check_executable(&candidate)
            && seen.insert(path.clone())
        {
            candidates.push(path);
        }
    }

    // Check known shim/stable locations (mise, asdf, nix)
    let shim_dirs = [
        dirs::home_dir().map(|h| h.join(".local/share/mise/shims")),
        dirs::home_dir().map(|h| h.join(".mise/shims")),
        dirs::home_dir().map(|h| h.join(".asdf/shims")),
        dirs::home_dir().map(|h| h.join(".nix-profile/bin")),
    ];

    for shim_dir in shim_dirs.into_iter().flatten() {
        let candidate = shim_dir.join(name);
        if let Some(path) = check_executable(&candidate)
            && seen.insert(path.clone())
        {
            candidates.push(path);
        }
    }

    candidates
}

/// Check if path is a known shim location
fn is_shim_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let shim_patterns = [
        "/mise/shims/",
        "/.mise/shims/",
        "/asdf/shims/",
        "/.asdf/shims/",
    ];
    shim_patterns.iter().any(|p| path_str.contains(p))
}

/// Compute simple hash of file for comparison
fn file_hash(path: &Path) -> Option<u64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    use std::io::Read;

    let mut file = fs::File::open(path).ok()?;
    let mut hasher = DefaultHasher::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.write(&buf[..n]);
    }

    Some(hasher.finish())
}

/// Check if path is an executable file, return the path as-is if valid
/// (Don't canonicalize to preserve shim paths)
fn check_executable(path: &Path) -> Option<PathBuf> {
    if !path.exists() || !path.is_file() {
        return None;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = path.metadata()
            && meta.permissions().mode() & 0o111 != 0
        {
            return Some(path.to_path_buf());
        }
        None
    }

    #[cfg(not(unix))]
    {
        Some(path.to_path_buf())
    }
}

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
            // Find all executable candidates, current first
            let mut candidates = vec![current_exe.clone()];
            for c in find_executable_candidates("authsock-filter") {
                if c != current_exe {
                    candidates.push(c);
                }
            }

            // Get canonical path of current exe for comparison
            let current_canonical = current_exe.canonicalize().ok();
            let current_hash = file_hash(&current_exe);

            let mut msg = format!(
                "Executable is under {} version manager: {}\n\nCandidates:\n",
                info.name,
                current_exe.display()
            );

            for (i, path) in candidates.iter().enumerate() {
                let mut positive_marks = Vec::new();
                let mut negative_marks = Vec::new();
                let is_current = path == &current_exe;
                let version_info = detect_version_manager(path);

                // Check if this is a known shim path
                let is_shim = is_shim_path(path);
                if is_shim {
                    positive_marks.push("shim");
                }

                // Check if this is the current executable (positive)
                if is_current {
                    positive_marks.push("current");
                } else if !is_shim {
                    // Check if same target (symlink resolves to same file)
                    let path_canonical = path.canonicalize().ok();
                    if path_canonical.is_some() && path_canonical == current_canonical {
                        positive_marks.push("same-target");
                    } else if let Some(ref ch) = current_hash {
                        // Check if same content (hash)
                        if file_hash(path).as_ref() == Some(ch) {
                            positive_marks.push("same-hash");
                        }
                    }
                }

                // Check if versioned or unstable path
                if let Some(ref vi) = version_info {
                    if vi.name == "temporary" {
                        negative_marks.push("unstable-path");
                    } else {
                        negative_marks.push("versioned-path");
                    }
                }

                // Build colored marker string
                let mut marker_parts = Vec::new();
                if !positive_marks.is_empty() {
                    marker_parts.push(format!("\x1b[32m{}\x1b[0m", positive_marks.join(", ")));
                }
                if !negative_marks.is_empty() {
                    marker_parts.push(format!("\x1b[31m{}\x1b[0m", negative_marks.join(", ")));
                }

                let marker = if marker_parts.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", marker_parts.join(", "))
                };

                // Highlight recommended paths (has positive marks, no negative marks)
                let is_recommended = !positive_marks.is_empty() && negative_marks.is_empty();
                let line = format!("  {}. {}{}", i + 1, path.display(), marker);
                if is_recommended {
                    msg.push_str(&format!("\x1b[32m{}\x1b[0m\n", line));
                } else {
                    msg.push_str(&format!("{}\n", line));
                }
            }

            msg.push_str("\nUse --executable <PATH> or --force");

            bail!("{}", msg);
        }
    }

    Ok(current_exe)
}

/// Get config file path (required for service registration)
fn get_config_path(config_override: Option<PathBuf>) -> Result<PathBuf> {
    config_override
        .or_else(find_config_file)
        .context("No configuration file found. Create ~/.config/authsock-filter/config.toml first.")
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

    pub fn generate_plist(name: &str, exe_path: &str, config_path: &str) -> Result<Vec<u8>> {
        let args = vec![
            exe_path.to_string(),
            "run".to_string(),
            "--config".to_string(),
            config_path.to_string(),
        ];

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

    pub fn generate_unit(_name: &str, exe_path: &str, config_path: &str) -> String {
        // Quote paths to handle spaces and special characters
        let exe_quoted = shell_quote(exe_path);
        let config_quoted = shell_quote(config_path);
        format!(
            r#"[Unit]
Description=SSH agent proxy with key filtering
After=default.target

[Service]
Type=simple
ExecStart={exe_quoted} run --config {config_quoted}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#
        )
    }

    /// Quote a string for systemd ExecStart (handles spaces and special chars)
    fn shell_quote(s: &str) -> String {
        if s.contains(|c: char| c.is_whitespace() || c == '"' || c == '\\') {
            // Escape backslashes and double quotes, then wrap in double quotes
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{}\"", escaped)
        } else {
            s.to_string()
        }
    }
}

// ============================================================================
// Public API - macOS
// ============================================================================

#[cfg(target_os = "macos")]
pub async fn register(args: RegisterArgs, config_override: Option<PathBuf>) -> Result<()> {
    let exe_path = resolve_service_executable(args.executable.clone(), args.force)?;
    let exe_path_str = exe_path.display().to_string();
    let config_path = get_config_path(config_override)?;
    let config_path_str = config_path.display().to_string();

    // Validate config file
    let config_file = load_config(&config_path)?;
    if config_file.config.sockets.is_empty() {
        bail!(
            "Configuration file has no sockets defined: {}",
            config_path.display()
        );
    }

    info!(name = %args.name, executable = %exe_path_str, config = %config_path_str, "Registering launchd service");

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
    let plist_content = launchd::generate_plist(&args.name, &exe_path_str, &config_path_str)?;
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
    println!("Config: {}", config_path.display());
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

#[cfg(target_os = "macos")]
pub async fn reload(args: UnregisterArgs) -> Result<()> {
    info!(name = %args.name, "Reloading launchd service");

    let plist_path = launchd::plist_path(&args.name);

    if !plist_path.exists() {
        bail!("Service is not registered. Use 'service register' first.");
    }

    // Unload and reload the service
    let _ = std::process::Command::new("launchctl")
        .args(["unload", plist_path.to_str().unwrap()])
        .status();

    let status = std::process::Command::new("launchctl")
        .args(["load", "-w", plist_path.to_str().unwrap()])
        .status()
        .context("Failed to reload service")?;

    if !status.success() {
        bail!("Failed to reload service");
    }

    println!("Service reloaded successfully!");
    Ok(())
}

// ============================================================================
// Public API - Linux
// ============================================================================

#[cfg(target_os = "linux")]
pub async fn register(args: RegisterArgs, config_override: Option<PathBuf>) -> Result<()> {
    let exe_path = resolve_service_executable(args.executable.clone(), args.force)?;
    let exe_path_str = exe_path.display().to_string();
    let config_path = get_config_path(config_override)?;
    let config_path_str = config_path.display().to_string();

    // Validate config file
    let config_file = load_config(&config_path)?;
    if config_file.config.sockets.is_empty() {
        bail!(
            "Configuration file has no sockets defined: {}",
            config_path.display()
        );
    }

    info!(name = %args.name, executable = %exe_path_str, config = %config_path_str, "Registering systemd service");

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
    let unit_content = systemd::generate_unit(&args.name, &exe_path_str, &config_path_str);
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
    println!("Config: {}", config_path.display());
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

#[cfg(target_os = "linux")]
pub async fn reload(args: UnregisterArgs) -> Result<()> {
    info!(name = %args.name, "Reloading systemd service");

    let unit_path = systemd::unit_path(&args.name);

    if !unit_path.exists() {
        bail!("Service is not registered. Use 'service register' first.");
    }

    let status = std::process::Command::new("systemctl")
        .args(["--user", "restart", &args.name])
        .status()
        .context("Failed to restart service")?;

    if !status.success() {
        bail!("Failed to restart service");
    }

    println!("Service reloaded successfully!");
    Ok(())
}

// ============================================================================
// Public API - Unsupported platforms
// ============================================================================

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn register(_args: RegisterArgs, _config_override: Option<PathBuf>) -> Result<()> {
    bail!("Service registration is not supported on this platform")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn unregister(_args: UnregisterArgs) -> Result<()> {
    bail!("Service management is not supported on this platform")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn reload(_args: UnregisterArgs) -> Result<()> {
    bail!("Service management is not supported on this platform")
}
