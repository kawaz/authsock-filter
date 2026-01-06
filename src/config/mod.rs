//! Configuration module for authsock-filter
//!
//! This module handles loading and parsing of configuration files,
//! including environment variable expansion and path resolution.

mod file;

use crate::utils::path::expand_path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub use file::{ConfigFile, ConfigPath, config_search_paths, find_config_file, load_config};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Path to the upstream SSH agent socket
    /// Supports environment variable expansion (e.g., $SSH_AUTH_SOCK)
    #[serde(default = "default_upstream")]
    pub upstream: String,

    /// Socket definitions
    #[serde(default)]
    pub sockets: HashMap<String, SocketConfig>,

    /// GitHub API settings
    #[serde(default)]
    pub github: GithubConfig,
}

/// Configuration for a single socket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketConfig {
    /// Path to the socket file
    /// Supports environment variable and tilde expansion
    pub path: String,

    /// Optional upstream for this socket (overrides global upstream)
    #[serde(default)]
    pub upstream: Option<String>,

    /// Filter rules for this socket
    /// Mixed format: strings are single OR terms, arrays are AND groups
    /// e.g., ["f1", "f2", ["f3", "f4"]] means f1 || f2 || (f3 && f4)
    #[serde(
        default,
        deserialize_with = "deserialize_filters",
        serialize_with = "serialize_filters"
    )]
    pub filters: Vec<Vec<String>>,
}

/// Custom deserializer for filters:
/// - `"f1"` → single filter (OR term)
/// - `["f1", "f2"]` → AND group
/// - `["f1", ["f2", "f3"]]` → f1 || (f2 && f3)
fn deserialize_filters<'de, D>(deserializer: D) -> Result<Vec<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, SeqAccess, Visitor};
    use std::fmt;

    struct FiltersVisitor;

    impl<'de> Visitor<'de> for FiltersVisitor {
        type Value = Vec<Vec<String>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a sequence of strings or arrays of strings")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut result = Vec::new();

            while let Some(value) = seq.next_element::<toml::Value>()? {
                match value {
                    toml::Value::String(s) => {
                        // Single string → single-element AND group (OR term)
                        result.push(vec![s]);
                    }
                    toml::Value::Array(arr) => {
                        // Array → AND group
                        let group: Vec<String> = arr
                            .into_iter()
                            .map(|v| {
                                v.as_str().map(|s| s.to_string()).ok_or_else(|| {
                                    de::Error::custom("expected string in filter group")
                                })
                            })
                            .collect::<Result<_, _>>()?;
                        result.push(group);
                    }
                    _ => {
                        return Err(de::Error::custom("expected string or array of strings"));
                    }
                }
            }

            Ok(result)
        }
    }

    deserializer.deserialize_seq(FiltersVisitor)
}

/// Custom serializer for filters:
/// - Single-element group → string (e.g., `["f1"]` → `"f1"`)
/// - Multi-element group → array (e.g., `["f1", "f2"]` → `["f1", "f2"]`)
fn serialize_filters<S>(filters: &Vec<Vec<String>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;

    let mut seq = serializer.serialize_seq(Some(filters.len()))?;
    for group in filters {
        if group.len() == 1 {
            // Single filter → serialize as string
            seq.serialize_element(&group[0])?;
        } else {
            // Multiple filters → serialize as array
            seq.serialize_element(group)?;
        }
    }
    seq.end()
}

/// GitHub API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    /// Cache TTL for GitHub API responses
    /// Format: "1h", "30m", "1d", etc.
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl: String,

    /// Timeout for GitHub API requests
    /// Format: "10s", "30s", etc.
    #[serde(default = "default_timeout")]
    pub timeout: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            upstream: default_upstream(),
            sockets: HashMap::new(),
            github: GithubConfig::default(),
        }
    }
}

impl Default for GithubConfig {
    fn default() -> Self {
        Self {
            cache_ttl: default_cache_ttl(),
            timeout: default_timeout(),
        }
    }
}

fn default_upstream() -> String {
    "$SSH_AUTH_SOCK".to_string()
}

fn default_cache_ttl() -> String {
    "1h".to_string()
}

fn default_timeout() -> String {
    "10s".to_string()
}

impl Config {
    /// Expand environment variables and tilde in all paths
    pub fn expand_paths(&self) -> crate::Result<ExpandedConfig> {
        let upstream = expand_path(&self.upstream)?;

        let mut sockets = HashMap::new();
        for (name, socket) in &self.sockets {
            let socket_upstream = socket
                .upstream
                .as_ref()
                .map(|u| expand_path(u))
                .transpose()?
                .map(PathBuf::from);

            sockets.insert(
                name.clone(),
                ExpandedSocketConfig {
                    path: PathBuf::from(expand_path(&socket.path)?),
                    upstream: socket_upstream,
                    filters: socket.filters.clone(),
                },
            );
        }

        Ok(ExpandedConfig {
            upstream: PathBuf::from(upstream),
            sockets,
            github: ExpandedGithubConfig {
                cache_ttl: parse_duration(&self.github.cache_ttl)?,
                timeout: parse_duration(&self.github.timeout)?,
            },
        })
    }
}

/// Configuration with all paths expanded
#[derive(Debug, Clone)]
pub struct ExpandedConfig {
    /// Resolved path to the upstream SSH agent socket
    pub upstream: PathBuf,

    /// Socket definitions with expanded paths
    pub sockets: HashMap<String, ExpandedSocketConfig>,

    /// GitHub API settings with parsed durations
    pub github: ExpandedGithubConfig,
}

/// Socket configuration with expanded path
#[derive(Debug, Clone)]
pub struct ExpandedSocketConfig {
    /// Resolved socket path
    pub path: PathBuf,

    /// Resolved upstream path (if overridden for this socket)
    pub upstream: Option<PathBuf>,

    /// Filter rules for this socket (outer: OR, inner: AND)
    pub filters: Vec<Vec<String>>,
}

/// GitHub configuration with parsed durations
#[derive(Debug, Clone)]
pub struct ExpandedGithubConfig {
    /// Cache TTL as Duration
    pub cache_ttl: std::time::Duration,

    /// Timeout as Duration
    pub timeout: std::time::Duration,
}

/// Parse a duration string like "1h", "30m", "10s", "1d"
pub fn parse_duration(s: &str) -> crate::Result<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return Err(crate::Error::Config("Empty duration string".to_string()));
    }

    // Find the position where the numeric part ends
    let (num_str, unit) = s
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map(|(i, _)| (&s[..i], &s[i..]))
        .unwrap_or((s, "s")); // Default to seconds if no unit

    let num: u64 = num_str.trim().parse().map_err(|e| {
        crate::Error::Config(format!("Invalid duration number '{}': {}", num_str, e))
    })?;

    let seconds = match unit.to_lowercase().as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => num,
        "m" | "min" | "mins" | "minute" | "minutes" => num * 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => num * 60 * 60,
        "d" | "day" | "days" => num * 60 * 60 * 24,
        "w" | "week" | "weeks" => num * 60 * 60 * 24 * 7,
        "" => num, // Assume seconds if no unit
        _ => {
            return Err(crate::Error::Config(format!(
                "Unknown duration unit '{}' in '{}'",
                unit, s
            )));
        }
    };

    Ok(std::time::Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(
            parse_duration("10s").unwrap(),
            std::time::Duration::from_secs(10)
        );
        assert_eq!(
            parse_duration("30sec").unwrap(),
            std::time::Duration::from_secs(30)
        );
        assert_eq!(
            parse_duration("5").unwrap(),
            std::time::Duration::from_secs(5)
        );
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(
            parse_duration("5m").unwrap(),
            std::time::Duration::from_secs(300)
        );
        assert_eq!(
            parse_duration("2min").unwrap(),
            std::time::Duration::from_secs(120)
        );
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(
            parse_duration("1h").unwrap(),
            std::time::Duration::from_secs(3600)
        );
        assert_eq!(
            parse_duration("2hours").unwrap(),
            std::time::Duration::from_secs(7200)
        );
    }

    #[test]
    fn test_parse_duration_days() {
        assert_eq!(
            parse_duration("1d").unwrap(),
            std::time::Duration::from_secs(86400)
        );
        assert_eq!(
            parse_duration("7days").unwrap(),
            std::time::Duration::from_secs(604800)
        );
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("10x").is_err());
    }

    #[test]
    fn test_expand_path_env_var() {
        // SAFETY: This test runs in isolation and TEST_VAR is not used elsewhere
        unsafe { std::env::set_var("TEST_VAR", "/test/path") };
        assert_eq!(
            expand_path("$TEST_VAR/socket").unwrap(),
            "/test/path/socket"
        );
        unsafe { std::env::remove_var("TEST_VAR") };
    }

    #[test]
    fn test_expand_path_tilde() {
        let result = expand_path("~/test").unwrap();
        assert!(result.starts_with('/'));
        assert!(result.ends_with("/test"));
        assert!(!result.starts_with('~'));
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.upstream, "$SSH_AUTH_SOCK");
        assert!(config.sockets.is_empty());
        assert_eq!(config.github.cache_ttl, "1h");
        assert_eq!(config.github.timeout, "10s");
    }

    #[test]
    fn test_parse_config_toml() {
        let toml_str = r#"
upstream = "$SSH_AUTH_SOCK"

[sockets.work]
path = "$XDG_RUNTIME_DIR/authsock-filter/work.sock"
filters = ["comment=~@work\\.example\\.com$"]

[sockets.personal]
path = "~/.ssh/personal-agent.sock"
filters = [
    ["github=kawaz", "type=ed25519"],
]

[github]
cache_ttl = "1h"
timeout = "10s"
"#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.upstream, "$SSH_AUTH_SOCK");
        assert_eq!(config.sockets.len(), 2);

        let work = config.sockets.get("work").unwrap();
        assert_eq!(work.path, "$XDG_RUNTIME_DIR/authsock-filter/work.sock");
        // Single AND group
        assert_eq!(
            work.filters,
            vec![vec!["comment=~@work\\.example\\.com$".to_string()]]
        );

        let personal = config.sockets.get("personal").unwrap();
        assert_eq!(personal.path, "~/.ssh/personal-agent.sock");
        // Single AND group with two filters
        assert_eq!(
            personal.filters,
            vec![vec!["github=kawaz".to_string(), "type=ed25519".to_string()]]
        );

        assert_eq!(config.github.cache_ttl, "1h");
        assert_eq!(config.github.timeout, "10s");
    }

    #[test]
    fn test_filters_serialize_deserialize_roundtrip() {
        // Test that filters maintain their format through serialization/deserialization
        let toml_str = r#"
path = "/tmp/test.sock"
filters = ["f1", "f2", ["f3", "f4"]]
"#;

        // Deserialize
        let config: SocketConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.filters,
            vec![
                vec!["f1".to_string()],
                vec!["f2".to_string()],
                vec!["f3".to_string(), "f4".to_string()],
            ]
        );

        // Serialize back
        let serialized = toml::to_string(&config).unwrap();

        // Deserialize again
        let config2: SocketConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config.filters, config2.filters);

        // Verify format is preserved (single-element groups as strings, multi-element as arrays)
        assert!(serialized.contains("\"f1\"") || serialized.contains("'f1'"));
        assert!(serialized.contains("\"f2\"") || serialized.contains("'f2'"));
    }

    #[test]
    fn test_filters_empty_serialization() {
        let config = SocketConfig {
            path: "/tmp/test.sock".to_string(),
            upstream: None,
            filters: vec![],
        };

        let serialized = toml::to_string(&config).unwrap();
        let config2: SocketConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config.filters, config2.filters);
    }
}
