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

/// Standard configuration file name
const CONFIG_FILE_NAME: &str = "config.toml";

/// Application name for directory paths
const APP_NAME: &str = "authsock-filter";

/// Find the configuration file in standard locations
///
/// Search order:
/// 1. `$XDG_CONFIG_HOME/authsock-filter/config.toml` (or `~/.config/authsock-filter/config.toml`)
/// 2. `~/.authsock-filter/config.toml`
/// 3. `~/.authsock-filter.toml`
///
/// Returns `None` if no configuration file is found.
pub fn find_config_file() -> Option<PathBuf> {
    let candidates = config_file_candidates();

    for path in candidates {
        if path.exists() && path.is_file() {
            tracing::debug!("Found configuration file at: {}", path.display());
            return Some(path);
        }
    }

    tracing::debug!("No configuration file found in standard locations");
    None
}

/// Get all candidate paths for configuration files
fn config_file_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // XDG config directory (or ~/.config)
    if let Some(config_dir) = dirs::config_dir() {
        candidates.push(config_dir.join(APP_NAME).join(CONFIG_FILE_NAME));
    }

    // Home directory based locations
    if let Some(home_dir) = dirs::home_dir() {
        // ~/.authsock-filter/config.toml
        candidates.push(
            home_dir
                .join(format!(".{}", APP_NAME))
                .join(CONFIG_FILE_NAME),
        );

        // ~/.authsock-filter.toml
        candidates.push(home_dir.join(format!(".{}.toml", APP_NAME)));
    }

    candidates
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
    fn test_config_file_candidates() {
        let candidates = config_file_candidates();
        assert!(!candidates.is_empty());

        // All candidates should be absolute paths
        for path in &candidates {
            assert!(path.is_absolute(), "Path should be absolute: {:?}", path);
        }

        // Check that expected patterns exist
        let has_xdg_config = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("authsock-filter/config.toml"));
        assert!(has_xdg_config, "Should have XDG config path");
    }

    #[test]
    fn test_load_config_valid() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let toml_content = r#"
upstream = "/run/user/1000/ssh-agent.sock"

[sockets.test]
path = "/tmp/test.sock"
filters = ["type:ed25519"]

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
        assert!(config_file.config.log_path.is_none()); // Default
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
log_path = "$XDG_STATE_HOME/authsock-filter/messages.jsonl"

[sockets.work]
path = "$XDG_RUNTIME_DIR/authsock-filter/work.sock"
filters = ["comment:~@work\\.example\\.com$"]

[sockets.personal]
path = "~/.ssh/personal-agent.sock"
filters = [
    "github:kawaz",
    "type:ed25519",
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
