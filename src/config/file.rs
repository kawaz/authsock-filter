//! Configuration file discovery and loading
//!
//! This module provides functionality to find and load configuration files
//! from standard locations.

use std::path::{Path, PathBuf};

use super::Config;

/// Configuration file wrapper with path information
#[derive(Debug, Clone)]
pub struct ConfigFile {
    /// Path where the configuration was loaded from
    pub path: PathBuf,

    /// The parsed configuration
    pub config: Config,
}

/// Configuration file search path with description
#[derive(Debug, Clone)]
pub struct ConfigPath {
    /// The actual file path
    pub path: PathBuf,
    /// Human-readable description for display
    pub description: &'static str,
}

/// Standard configuration file name
const CONFIG_FILE_NAME: &str = "config.toml";

/// Application name for directory paths
const APP_NAME: &str = "authsock-filter";

/// Get all configuration search paths with descriptions (in priority order)
///
/// Search order:
/// 1. `$XDG_CONFIG_HOME/authsock-filter/config.toml` (if env var set)
/// 2. `~/Library/Application Support/authsock-filter/config.toml` (macOS)
/// 3. `~/.config/authsock-filter/config.toml` (cross-platform fallback)
/// 4. `~/.authsock-filter/config.toml`
/// 5. `~/.authsock-filter.toml`
/// 6. `/etc/authsock-filter/config.toml` (Unix system-wide)
pub fn config_search_paths() -> Vec<ConfigPath> {
    let mut paths = Vec::new();

    // 1. XDG_CONFIG_HOME (explicit env var takes priority)
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        paths.push(ConfigPath {
            path: PathBuf::from(xdg).join(APP_NAME).join(CONFIG_FILE_NAME),
            description: "$XDG_CONFIG_HOME/authsock-filter/config.toml",
        });
    }

    // 2. Platform-specific config directory
    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        paths.push(ConfigPath {
            path: home
                .join("Library/Application Support")
                .join(APP_NAME)
                .join(CONFIG_FILE_NAME),
            description: "~/Library/Application Support/authsock-filter/config.toml",
        });
    }

    #[cfg(target_os = "linux")]
    if std::env::var("XDG_CONFIG_HOME").is_err() {
        if let Some(home) = dirs::home_dir() {
            paths.push(ConfigPath {
                path: home.join(".config").join(APP_NAME).join(CONFIG_FILE_NAME),
                description: "~/.config/authsock-filter/config.toml",
            });
        }
    }

    // 3. ~/.config fallback (cross-platform, avoid duplicate)
    if let Some(home) = dirs::home_dir() {
        let dotconfig = home.join(".config").join(APP_NAME).join(CONFIG_FILE_NAME);
        if !paths.iter().any(|p| p.path == dotconfig) {
            paths.push(ConfigPath {
                path: dotconfig,
                description: "~/.config/authsock-filter/config.toml",
            });
        }
    }

    // 4-5. Home directory based locations
    if let Some(home) = dirs::home_dir() {
        paths.push(ConfigPath {
            path: home.join(format!(".{}", APP_NAME)).join(CONFIG_FILE_NAME),
            description: "~/.authsock-filter/config.toml",
        });
        paths.push(ConfigPath {
            path: home.join(format!(".{}.toml", APP_NAME)),
            description: "~/.authsock-filter.toml",
        });
    }

    // 6. System-wide (Unix only)
    #[cfg(unix)]
    {
        paths.push(ConfigPath {
            path: PathBuf::from("/etc").join(APP_NAME).join(CONFIG_FILE_NAME),
            description: "/etc/authsock-filter/config.toml",
        });
    }

    paths
}

/// Find the configuration file in standard locations
///
/// Returns `None` if no configuration file is found.
pub fn find_config_file() -> Option<PathBuf> {
    for cp in config_search_paths() {
        if cp.path.exists() && cp.path.is_file() {
            tracing::info!(path = %cp.path.display(), "Found configuration file");
            return Some(cp.path);
        }
    }

    tracing::debug!("No configuration file found in standard locations");
    None
}

/// Load configuration from the specified path
pub fn load_config(path: &Path) -> crate::Result<ConfigFile> {
    tracing::debug!("Loading configuration from: {}", path.display());

    let content = std::fs::read_to_string(path).map_err(|e| {
        crate::Error::Config(format!(
            "Failed to read configuration file '{}': {}",
            path.display(),
            e
        ))
    })?;

    let config: Config = toml::from_str(&content).map_err(|e| {
        crate::Error::Config(format!(
            "Failed to parse configuration file '{}': {}",
            path.display(),
            e
        ))
    })?;

    Ok(ConfigFile {
        path: path.to_path_buf(),
        config,
    })
}

/// Load configuration from the first found standard location
///
/// Returns the default configuration if no file is found.
#[allow(dead_code)]
pub fn load_config_from_default_location() -> crate::Result<ConfigFile> {
    match find_config_file() {
        Some(path) => load_config(&path),
        None => {
            tracing::info!("No configuration file found, using defaults");
            Ok(ConfigFile {
                path: PathBuf::new(),
                config: Config::default(),
            })
        }
    }
}

/// Load configuration from a specific path or fall back to default locations
#[allow(dead_code)]
pub fn load_config_from_path_or_default(path: Option<&Path>) -> crate::Result<ConfigFile> {
    match path {
        Some(p) => load_config(p),
        None => load_config_from_default_location(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_search_paths() {
        let paths = config_search_paths();
        assert!(!paths.is_empty());

        // All paths should be absolute
        for cp in &paths {
            assert!(
                cp.path.is_absolute(),
                "Path should be absolute: {:?}",
                cp.path
            );
            assert!(
                !cp.description.is_empty(),
                "Description should not be empty"
            );
        }

        // Check that expected patterns exist
        let has_config_path = paths.iter().any(|p| {
            p.path
                .to_string_lossy()
                .contains("authsock-filter/config.toml")
        });
        assert!(has_config_path, "Should have config path");
    }

    #[test]
    fn test_load_config_valid() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let toml_content = r#"
upstream = "/run/user/1000/ssh-agent.sock"

[sockets.test]
path = "/tmp/test.sock"
filters = ["type=ed25519"]

[github]
cache_ttl = "2h"
timeout = "30s"
"#;

        std::fs::write(&config_path, toml_content).unwrap();

        let config_file = load_config(&config_path).unwrap();
        assert_eq!(config_file.path, config_path);
        assert_eq!(config_file.config.upstream, "/run/user/1000/ssh-agent.sock");
        assert_eq!(config_file.config.sockets.len(), 1);
        assert!(config_file.config.sockets.contains_key("test"));
        assert_eq!(config_file.config.github.cache_ttl, "2h");
        assert_eq!(config_file.config.github.timeout, "30s");
    }

    #[test]
    fn test_load_config_minimal() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Minimal config - just use defaults
        let toml_content = r#"
[sockets.minimal]
path = "/tmp/minimal.sock"
"#;

        std::fs::write(&config_path, toml_content).unwrap();

        let config_file = load_config(&config_path).unwrap();
        assert_eq!(config_file.config.upstream, "$SSH_AUTH_SOCK"); // Default
        assert_eq!(config_file.config.github.cache_ttl, "1h"); // Default
        assert_eq!(config_file.config.github.timeout, "10s"); // Default
    }

    #[test]
    fn test_load_config_not_found() {
        let result = load_config(Path::new("/nonexistent/path/config.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, crate::Error::Config(_)));
    }

    #[test]
    fn test_load_config_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        std::fs::write(&config_path, "invalid toml { [ }").unwrap();

        let result = load_config(&config_path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, crate::Error::Config(_)));
    }

    #[test]
    fn test_load_config_unknown_field() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let toml_content = r#"
upstream = "/tmp/socket"
unknown_field = "value"
"#;

        std::fs::write(&config_path, toml_content).unwrap();

        let result = load_config(&config_path);
        assert!(result.is_err(), "Should reject unknown fields");
    }

    #[test]
    fn test_load_config_with_all_options() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let toml_content = r#"
upstream = "$SSH_AUTH_SOCK"

[sockets.work]
path = "$XDG_RUNTIME_DIR/authsock-filter/work.sock"
filters = ["comment=~@work\\.example\\.com$"]

[sockets.personal]
path = "~/.ssh/personal-agent.sock"
filters = [
    "github=kawaz",
    "type=ed25519",
]

[github]
cache_ttl = "1h"
timeout = "10s"
"#;

        std::fs::write(&config_path, toml_content).unwrap();

        let config_file = load_config(&config_path).unwrap();
        assert_eq!(config_file.config.sockets.len(), 2);

        let work = config_file.config.sockets.get("work").unwrap();
        assert_eq!(work.filters.len(), 1);

        let personal = config_file.config.sockets.get("personal").unwrap();
        assert_eq!(personal.filters.len(), 2);
    }
}
