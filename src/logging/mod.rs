//! Logging module for authsock-filter
//!
//! This module provides logging functionality using tracing and tracing-subscriber.
//! It supports:
//! - Configurable log levels via verbose/quiet flags
//! - JSONL file output for structured logging
//! - Stderr output for human-readable logs

pub mod jsonl;

pub use jsonl::{Decision, JsonlWriter, LogEvent, LogEventKind};

use std::path::Path;
use tracing::Level;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

/// Logging configuration
#[derive(Debug, Clone, Default)]
pub struct LogConfig {
    /// Verbosity level adjustment: -1 for quiet, 0 for normal, +1 for verbose
    pub verbosity: i8,
    /// Optional path to JSONL log file
    pub jsonl_path: Option<String>,
}

impl LogConfig {
    /// Create a new log configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set verbose mode (+1 verbosity)
    pub fn verbose(mut self) -> Self {
        self.verbosity = 1;
        self
    }

    /// Set quiet mode (-1 verbosity)
    pub fn quiet(mut self) -> Self {
        self.verbosity = -1;
        self
    }

    /// Set JSONL output path
    pub fn with_jsonl_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.jsonl_path = Some(path.as_ref().to_string_lossy().to_string());
        self
    }

    /// Get the minimum log level based on verbosity
    fn min_level(&self) -> Level {
        match self.verbosity {
            v if v < 0 => Level::WARN, // quiet: only warnings and errors
            0 => Level::INFO,          // normal: info and above
            _ => Level::DEBUG,         // verbose: debug and above
        }
    }
}

/// Initialize the logging subsystem
///
/// # Arguments
/// * `verbose` - Enable verbose (debug) logging
/// * `quiet` - Enable quiet mode (warnings and errors only)
///
/// # Returns
/// A guard that must be kept alive for logging to work.
/// When using JSONL output, this ensures the file is properly flushed.
pub fn init(verbose: bool, quiet: bool) -> LogGuard {
    let config = LogConfig {
        verbosity: if quiet {
            -1
        } else if verbose {
            1
        } else {
            0
        },
        jsonl_path: None,
    };
    init_with_config(config)
}

/// Initialize logging with full configuration
///
/// # Arguments
/// * `config` - The logging configuration
///
/// # Returns
/// A guard that must be kept alive for logging to work.
pub fn init_with_config(config: LogConfig) -> LogGuard {
    let level = config.min_level();

    // Build the env filter
    // Allow overriding via RUST_LOG environment variable
    let env_filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy();

    // Create stderr layer with appropriate format
    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false);

    // Initialize JSONL writer if path is configured
    let jsonl_writer = config.jsonl_path.as_ref().and_then(|path| {
        JsonlWriter::new(path)
            .map_err(|e| {
                eprintln!("Warning: Failed to open JSONL log file '{}': {}", path, e);
            })
            .ok()
    });

    // Build and set the subscriber
    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer);

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global tracing subscriber");

    LogGuard {
        _jsonl_writer: jsonl_writer,
    }
}

/// Guard that keeps logging resources alive
///
/// This guard must be kept alive for the duration of the program.
/// When dropped, it ensures that any buffered logs are flushed.
#[must_use = "LogGuard must be kept alive for logging to work"]
pub struct LogGuard {
    _jsonl_writer: Option<JsonlWriter>,
}

impl LogGuard {
    /// Get a reference to the JSONL writer, if configured
    pub fn jsonl_writer(&self) -> Option<&JsonlWriter> {
        self._jsonl_writer.as_ref()
    }

    /// Write a log event to the JSONL file
    pub fn log_event(&self, event: &LogEvent) {
        if let Some(writer) = &self._jsonl_writer {
            if let Err(e) = writer.write(event) {
                tracing::warn!("Failed to write JSONL log event: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert_eq!(config.verbosity, 0);
        assert!(config.jsonl_path.is_none());
    }

    #[test]
    fn test_log_config_verbose() {
        let config = LogConfig::new().verbose();
        assert_eq!(config.verbosity, 1);
        assert_eq!(config.min_level(), Level::DEBUG);
    }

    #[test]
    fn test_log_config_quiet() {
        let config = LogConfig::new().quiet();
        assert_eq!(config.verbosity, -1);
        assert_eq!(config.min_level(), Level::WARN);
    }

    #[test]
    fn test_log_config_normal() {
        let config = LogConfig::new();
        assert_eq!(config.min_level(), Level::INFO);
    }
}
