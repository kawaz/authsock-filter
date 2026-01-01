//! Error types for authsock-filter

use thiserror::Error;

/// Main error type for authsock-filter
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Filter error: {0}")]
    Filter(String),

    #[error("SSH key error: {0}")]
    SshKey(#[from] ssh_key::Error),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Upstream agent not available: {0}")]
    UpstreamNotAvailable(String),

    #[error("Socket error: {0}")]
    Socket(String),

    #[error("Daemon error: {0}")]
    Daemon(String),

    #[error("{0}")]
    Other(String),
}

/// Result type alias using our Error type
pub type Result<T> = std::result::Result<T, Error>;
