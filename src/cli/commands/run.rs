//! Run command - execute the proxy in the foreground

use anyhow::{bail, Context, Result};
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::signal;
use tracing::{debug, error, info, warn};

use crate::agent::{Proxy, Upstream};
use crate::cli::args::RunArgs;
use crate::filter::FilterEvaluator;
use crate::logging::jsonl::{JsonlWriter, LogEvent};

/// Execute the run command
pub async fn execute(args: RunArgs) -> Result<()> {
    // Parse upstream groups from command line arguments
    let upstream_groups = args.parse_upstream_groups();

    if upstream_groups.is_empty() {
        bail!("No upstream groups configured. Use --upstream and --socket to define proxy configuration.");
    }

    // Count total sockets
    let total_sockets: usize = upstream_groups.iter().map(|g| g.sockets.len()).sum();

    info!(
        upstream_count = upstream_groups.len(),
        socket_count = total_sockets,
        "Starting authsock-filter"
    );

    // Log configuration
    for group in &upstream_groups {
        info!(
            upstream = %group.path.display(),
            sockets = group.sockets.len(),
            "Upstream group"
        );
        for spec in &group.sockets {
            info!(
                socket = %spec.path.display(),
                filters = ?spec.filters,
                "  Configured socket"
            );
        }
    }

    // Set up JSONL logger if specified
    let logger: Option<Arc<JsonlWriter>> = if let Some(log_path) = &args.log {
        info!(log = %log_path.display(), "JSONL logging enabled");
        let writer = JsonlWriter::new(log_path)
            .context(format!("Failed to create log file: {}", log_path.display()))?;
        Some(Arc::new(writer))
    } else {
        None
    };

    // Start proxy servers for each upstream group
    let mut handles = Vec::new();
    let mut listeners = Vec::new();

    for group in &upstream_groups {
        // Validate upstream socket exists
        if !group.path.exists() {
            bail!("Upstream socket does not exist: {}", group.path.display());
        }

        // Create upstream connection manager for this group
        let upstream = Arc::new(Upstream::new(group.path.to_string_lossy().to_string()));

        for spec in &group.sockets {
            // Parse filters
            let filter = FilterEvaluator::parse(&spec.filters)
                .context(format!("Failed to parse filters for socket {}", spec.path.display()))?;

            // Ensure async filters are loaded (e.g., GitHub keys)
            filter.ensure_loaded().await.context(format!(
                "Failed to load filter data for socket {}",
                spec.path.display()
            ))?;

            let socket_path_str = spec.path.to_string_lossy().to_string();

            // Create proxy with optional logger
            let mut proxy = Proxy::new_shared(upstream.clone(), Arc::new(filter))
                .with_socket_path(&socket_path_str);

            if let Some(ref log) = logger {
                proxy = proxy.with_logger(log.clone());
                // Log server start
                if let Err(e) = log.write(&LogEvent::server_start(&socket_path_str)) {
                    warn!(error = %e, "Failed to write server start log");
                }
            }

            let proxy = Arc::new(proxy);

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

            info!(
                path = %spec.path.display(),
                upstream = %group.path.display(),
                "Listening on socket"
            );

            let socket_path = spec.path.clone();
            listeners.push((socket_path.clone(), socket_path_str.clone()));

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
    }

    info!(
        count = handles.len(),
        "Proxy server started. Press Ctrl+C to stop."
    );

    // Wait for shutdown signal
    signal::ctrl_c()
        .await
        .context("Failed to listen for shutdown signal")?;

    info!("Received shutdown signal, stopping...");

    // Cancel all listener tasks
    for handle in handles {
        handle.abort();
    }

    // Clean up socket files and log server stop
    for (path, path_str) in listeners {
        if let Some(ref log) = logger {
            if let Err(e) = log.write(&LogEvent::server_stop(&path_str)) {
                warn!(error = %e, "Failed to write server stop log");
            }
        }

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
