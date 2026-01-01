//! Run command - execute the proxy in the foreground

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use tokio::signal;
use tracing::{info, warn};

use crate::cli::args::RunArgs;

/// Socket specification parsed from command line
#[derive(Debug, Clone)]
pub struct SocketSpec {
    /// Path to the socket file
    pub path: PathBuf,
    /// Filter specifications
    pub filters: Vec<String>,
}

impl SocketSpec {
    /// Parse a socket specification string
    ///
    /// Format: /path/to/socket.sock:filter1:filter2...
    pub fn parse(spec: &str) -> Result<Self> {
        let parts: Vec<&str> = spec.splitn(2, ':').collect();
        let path = PathBuf::from(parts[0]);

        let filters = if parts.len() > 1 {
            parts[1]
                .split(':')
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        Ok(Self { path, filters })
    }
}

/// Execute the run command
pub async fn execute(args: RunArgs) -> Result<()> {
    // Validate upstream socket
    let upstream = args
        .upstream
        .as_ref()
        .context("Upstream socket path is required. Set SSH_AUTH_SOCK or use --upstream")?;

    if !upstream.exists() {
        bail!("Upstream socket does not exist: {}", upstream.display());
    }

    // Parse socket specifications
    let socket_specs: Vec<SocketSpec> = args
        .sockets
        .iter()
        .map(|s| SocketSpec::parse(s))
        .collect::<Result<Vec<_>>>()
        .context("Failed to parse socket specifications")?;

    if socket_specs.is_empty() {
        warn!("No socket specifications provided. Use -s/--socket to define filtered sockets.");
    }

    info!(
        upstream = %upstream.display(),
        sockets = socket_specs.len(),
        "Starting authsock-filter"
    );

    // Log configuration
    for spec in &socket_specs {
        info!(
            socket = %spec.path.display(),
            filters = ?spec.filters,
            "Configured socket"
        );
    }

    // Set up log file if specified
    if let Some(log_path) = &args.log {
        info!(log = %log_path.display(), "JSONL logging enabled");
        // TODO: Initialize JSONL logger
    }

    // TODO: Start the proxy server
    // This is where the actual proxy implementation would go:
    // 1. Create listener sockets for each socket_spec
    // 2. Accept connections
    // 3. Proxy requests to upstream, applying filters
    // 4. Log operations

    info!("Proxy server started. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    signal::ctrl_c()
        .await
        .context("Failed to listen for shutdown signal")?;

    info!("Received shutdown signal, stopping...");

    // TODO: Graceful shutdown
    // 1. Stop accepting new connections
    // 2. Wait for existing connections to complete
    // 3. Clean up socket files

    info!("Shutdown complete");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_spec_parse_simple() {
        let spec = SocketSpec::parse("/tmp/test.sock").unwrap();
        assert_eq!(spec.path, PathBuf::from("/tmp/test.sock"));
        assert!(spec.filters.is_empty());
    }

    #[test]
    fn test_socket_spec_parse_with_filters() {
        let spec = SocketSpec::parse("/tmp/test.sock:fingerprint:SHA256:xxx").unwrap();
        assert_eq!(spec.path, PathBuf::from("/tmp/test.sock"));
        assert_eq!(spec.filters, vec!["fingerprint", "SHA256", "xxx"]);
    }

    #[test]
    fn test_socket_spec_parse_github_filter() {
        let spec = SocketSpec::parse("/tmp/github.sock:github:kawaz").unwrap();
        assert_eq!(spec.path, PathBuf::from("/tmp/github.sock"));
        assert_eq!(spec.filters, vec!["github", "kawaz"]);
    }
}
