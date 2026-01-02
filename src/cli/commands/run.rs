//! Run command - execute the proxy in the foreground

use anyhow::{bail, Context, Result};
use std::sync::Arc;
use tokio::signal;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::agent::{Proxy, Server, Upstream};
use crate::cli::args::{RunArgs, SocketSpec};
use crate::filter::FilterEvaluator;

/// A running socket server with its proxy configuration
struct SocketServer {
    server: Server,
    proxy: Arc<Proxy>,
}

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
        // TODO: Initialize JSONL logger for request/response logging
    }

    // Create upstream connection manager
    let upstream = Arc::new(Upstream::new(upstream_path));

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Create and bind servers for each socket spec
    let mut socket_servers = Vec::new();
    for spec in &socket_specs {
        match create_socket_server(spec, Arc::clone(&upstream)).await {
            Ok(server) => socket_servers.push(server),
            Err(e) => {
                error!(
                    socket = %spec.path.display(),
                    error = %e,
                    "Failed to create socket server"
                );
                // Clean up already bound sockets on failure
                drop(socket_servers);
                return Err(e);
            }
        }
    }

    info!(
        count = socket_servers.len(),
        "Proxy server started. Press Ctrl+C to stop."
    );

    // Spawn server tasks
    let mut handles = Vec::new();
    for socket_server in socket_servers {
        let shutdown_rx = shutdown_rx.clone();
        let handle = tokio::spawn(async move {
            run_socket_server(socket_server, shutdown_rx).await
        });
        handles.push(handle);
    }

    // Wait for shutdown signal
    signal::ctrl_c()
        .await
        .context("Failed to listen for shutdown signal")?;

    info!("Received shutdown signal, stopping...");

    // Send shutdown signal to all servers
    let _ = shutdown_tx.send(true);

    // Wait for all servers to shut down gracefully
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                debug!(error = %e, "Server task finished with error");
            }
            Err(e) => {
                warn!(error = %e, "Server task panicked");
            }
        }
    }

    info!("Shutdown complete");

    Ok(())
}

/// Create a socket server for a given spec
async fn create_socket_server(spec: &SocketSpec, upstream: Arc<Upstream>) -> Result<SocketServer> {
    // Parse filters
    let filter = FilterEvaluator::parse(&spec.filters)
        .with_context(|| format!("Failed to parse filters for socket {}", spec.path.display()))?;

    // Ensure async filters are loaded (e.g., GitHub keys)
    filter.ensure_loaded().await.with_context(|| {
        format!(
            "Failed to load filter data for socket {}",
            spec.path.display()
        )
    })?;

    // Create proxy
    let proxy = Arc::new(Proxy::new_shared(upstream, Arc::new(filter)));

    // Create and bind server
    let mut server = Server::new(&spec.path);
    server.bind().await.with_context(|| {
        format!(
            "Failed to bind socket at {}",
            spec.path.display()
        )
    })?;

    Ok(SocketServer { server, proxy })
}

/// Run a socket server until shutdown
async fn run_socket_server(
    socket_server: SocketServer,
    shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let SocketServer { server, proxy } = socket_server;
    let socket_path = server.socket_path().to_path_buf();

    debug!(socket = %socket_path.display(), "Starting server loop");

    let result = server
        .run(
            move |stream| {
                let proxy = Arc::clone(&proxy);
                async move {
                    proxy.handle_client(stream).await.map_err(|e| e.into())
                }
            },
            shutdown_rx,
        )
        .await;

    debug!(socket = %socket_path.display(), "Server loop finished");

    result.map_err(|e| e.into())
}

