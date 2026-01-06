//! Config command - show or validate configuration

use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::info;

use crate::cli::args::ConfigArgs;

/// Configuration file search path with description
struct ConfigPath {
    path: PathBuf,
    description: &'static str,
}

/// Default configuration file paths to search (in priority order)
fn default_config_paths() -> Vec<ConfigPath> {
    let mut paths = Vec::new();

    // 1. XDG_CONFIG_HOME (explicit env var takes priority)
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let xdg_path = PathBuf::from(xdg);
        paths.push(ConfigPath {
            path: xdg_path.join("authsock-filter/config.toml"),
            description: "$XDG_CONFIG_HOME/authsock-filter/config.toml",
        });
    }

    // 2. Platform-specific config directory
    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        paths.push(ConfigPath {
            path: home.join("Library/Application Support/authsock-filter/config.toml"),
            description: "~/Library/Application Support/authsock-filter/config.toml (macOS)",
        });
    }

    #[cfg(target_os = "linux")]
    if let Some(home) = dirs::home_dir() {
        // Only add if XDG_CONFIG_HOME was not set (avoid duplicate)
        if std::env::var("XDG_CONFIG_HOME").is_err() {
            paths.push(ConfigPath {
                path: home.join(".config/authsock-filter/config.toml"),
                description: "~/.config/authsock-filter/config.toml (Linux default)",
            });
        }
    }

    // 3. ~/.config fallback (cross-platform)
    if let Some(home) = dirs::home_dir() {
        let dotconfig = home.join(".config/authsock-filter/config.toml");
        if !paths.iter().any(|p| p.path == dotconfig) {
            paths.push(ConfigPath {
                path: dotconfig,
                description: "~/.config/authsock-filter/config.toml",
            });
        }
    }

    // 4. Home directory dotfile
    if let Some(home) = dirs::home_dir() {
        paths.push(ConfigPath {
            path: home.join(".authsock-filter.toml"),
            description: "~/.authsock-filter.toml",
        });
    }

    // 5. System-wide (Unix only)
    #[cfg(unix)]
    {
        paths.push(ConfigPath {
            path: PathBuf::from("/etc/authsock-filter/config.toml"),
            description: "/etc/authsock-filter/config.toml (system-wide)",
        });
    }

    paths
}

/// Get just the paths for searching
fn config_paths() -> Vec<PathBuf> {
    default_config_paths().into_iter().map(|p| p.path).collect()
}

/// Find the first existing configuration file
fn find_config_file() -> Option<PathBuf> {
    config_paths().into_iter().find(|path| path.exists())
}

/// Example configuration content
fn example_config() -> &'static str {
    r#"# authsock-filter configuration file
#
# See https://github.com/kawaz/authsock-filter for documentation

# Default upstream SSH agent socket (used when socket doesn't specify one)
# Default: $SSH_AUTH_SOCK
# upstream = "/run/user/1000/ssh-agent.sock"

# Socket definitions
# Each socket can specify its own upstream and filters
[[sockets]]
path = "/tmp/authsock-filter/default.sock"
# upstream = "/path/to/agent.sock"  # Optional: override default upstream
# filters = ["github=username", "type=ed25519"]

# Example: Allow only GitHub keys for a specific user
# [[sockets]]
# path = "/tmp/authsock-filter/github.sock"
# filters = ["github=kawaz"]

# Example: Allow only ED25519 keys with comment pattern
# [[sockets]]
# path = "/tmp/authsock-filter/work.sock"
# filters = ["comment=*@work.example.com", "type=ed25519"]

# Example: Exclude DSA keys
# [[sockets]]
# path = "/tmp/authsock-filter/no-dsa.sock"
# filters = ["not-type=dsa"]

# Filter syntax:
#   type=value      Include keys matching the filter
#   not-type=value   Exclude keys matching the filter
#
# Filter types:
#   fingerprint=SHA256:xxx   Match by key fingerprint
#   comment=*pattern*        Match by comment (glob pattern)
#   github=username          Match keys from github.com/username.keys
#   type=ed25519|rsa|...     Match by key type
#   pubkey=<base64>          Match by full public key
#   keyfile=/path/to/file    Match keys from file
"#
}

/// Execute the config command
pub async fn execute(args: ConfigArgs) -> Result<()> {
    // Show search paths
    if args.paths {
        println!("Config search paths (in priority order):");
        for (i, cp) in default_config_paths().iter().enumerate() {
            let exists = if cp.path.exists() {
                "\x1b[32m[exists]\x1b[0m"
            } else {
                ""
            };
            println!("  {}. {} {}", i + 1, cp.description, exists);
            println!("     {}", cp.path.display());
        }
        return Ok(());
    }

    if args.example {
        // Show example configuration
        match args.format.as_str() {
            "json" => {
                // Convert TOML to JSON
                let config: toml::Value =
                    toml::from_str(example_config()).context("Failed to parse example config")?;
                let json = serde_json::to_string_pretty(&config)?;
                println!("{}", json);
            }
            _ => {
                print!("{}", example_config());
            }
        }
        return Ok(());
    }

    // Find and read configuration file
    let config_path = find_config_file();

    if let Some(path) = &config_path {
        info!(path = %path.display(), "Found configuration file");

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        if args.validate {
            // Validate configuration
            match toml::from_str::<toml::Value>(&content) {
                Ok(_) => {
                    println!("Configuration file is valid: {}", path.display());
                    // TODO: Add semantic validation (check socket paths, filter syntax, etc.)
                }
                Err(e) => {
                    eprintln!("Configuration file is invalid: {}", path.display());
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        } else {
            // Show configuration
            match args.format.as_str() {
                "json" => {
                    let config: toml::Value =
                        toml::from_str(&content).context("Failed to parse config")?;
                    let json = serde_json::to_string_pretty(&config)?;
                    println!("{}", json);
                }
                _ => {
                    println!("# Configuration from: {}", path.display());
                    println!();
                    print!("{}", content);
                }
            }
        }
    } else if args.validate {
        eprintln!("No configuration file found.");
        eprintln!("Searched locations (use --paths for details):");
        for cp in default_config_paths() {
            eprintln!("  - {}", cp.path.display());
        }
        std::process::exit(1);
    } else {
        println!("# No configuration file found");
        println!("# Use 'config --paths' to see search locations");
        println!();
        println!("# Example configuration (use --example for clean output):");
        println!();
        print!("{}", example_config());
    }

    Ok(())
}
