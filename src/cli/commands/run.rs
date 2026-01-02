//! Run command - execute the proxy in the foreground

use anyhow::{bail, Context, Result};
use tokio::signal;
use tracing::{info, warn};

use crate::cli::args::RunArgs;

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

    // Parse socket specifications from --socket and --filter arguments
    let socket_specs = args.parse_socket_specs();

    if socket_specs.is_empty() {
        warn!("No socket specifications provided. Use --socket and --filter to define filtered sockets.");
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

