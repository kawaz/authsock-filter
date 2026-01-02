//! Run command - execute the proxy in the foreground

use anyhow::{bail, Context, Result};
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::signal;
use tracing::{debug, error, info, warn};

use crate::agent::{Proxy, Upstream};
use crate::cli::args::RunArgs;
use crate::filter::FilterEvaluator;

/// Execute the run command
pub async fn execute(args: RunArgs) -> Result<()> {
    // Validate upstream socket
    let upstream_path = args
        .upstream
        .as_ref()
        .context("Upstream socket path is required. Set SSH_AUTH_SOCK or use --upstream")?;

    if !upstream_path.exists() {
        bail!("Upstream socket does not exist: {}", upstream_path.display());
    }

    // Parse socket specifications from --socket and --filter arguments
    let socket_specs = args.parse_socket_specs();

    if socket_specs.is_empty() {
        warn!("No socket specifications provided. Use --socket and --filter to define filtered sockets.");
    }

    info!(
        upstream = %upstream_path.display(),
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

    // Create upstream connection manager
    let upstream = Arc::new(Upstream::new(upstream_path.to_string_lossy().to_string()));

    // Start proxy servers for each socket spec
    let mut handles = Vec::new();
    let mut listeners = Vec::new();

    for spec in &socket_specs {
        // Parse filters
        let filter = FilterEvaluator::parse(&spec.filters)
            .context(format!("Failed to parse filters for socket {}", spec.path.display()))?;

        // Create proxy
        let proxy = Arc::new(Proxy::new_shared(upstream.clone(), Arc::new(filter)));

        // Remove existing socket if present
        if spec.path.exists() {
            debug!(path = %spec.path.display(), "Removing existing socket file");
            std::fs::remove_file(&spec.path)
                .context(format!("Failed to remove existing socket at {}", spec.path.display()))?;
        }

        // Ensure parent directory exists
        if let Some(parent) = spec.path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .context(format!("Failed to create directory {}", parent.display()))?;
            }
        }

        // Bind listener
        let listener = UnixListener::bind(&spec.path)
            .context(format!("Failed to bind to socket {}", spec.path.display()))?;

        info!(path = %spec.path.display(), "Listening on socket");

        let socket_path = spec.path.clone();
        listeners.push(socket_path.clone());

        // Spawn task to handle connections
        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let proxy = proxy.clone();
                        tokio::spawn(async move {
                            if let Err(e) = proxy.handle_client(stream).await {
                                debug!(error = %e, "Client connection error");
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to accept connection");
                        break;
                    }
                }
            }
        });

        handles.push(handle);
    }

    info!("Proxy server started. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    signal::ctrl_c()
        .await
        .context("Failed to listen for shutdown signal")?;

    info!("Received shutdown signal, stopping...");

    // Cancel all listener tasks
    for handle in handles {
        handle.abort();
    }

    // Clean up socket files
    for path in listeners {
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                warn!(path = %path.display(), error = %e, "Failed to remove socket file");
            } else {
                debug!(path = %path.display(), "Removed socket file");
            }
        }
    }

    info!("Shutdown complete");

    Ok(())
}
