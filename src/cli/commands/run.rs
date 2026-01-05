//! Run command - execute the proxy in the foreground

use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::signal;
use tracing::{debug, error, info};

use crate::agent::{Proxy, Upstream};
use crate::cli::args::RunArgs;
use crate::config::{Config, ExpandedConfig, SocketConfig, find_config_file, load_config};
use crate::filter::FilterEvaluator;

/// Execute the run command
pub async fn execute(args: RunArgs, config_path: Option<PathBuf>) -> Result<()> {
    // Handle --print-config: generate config from CLI args and print
    if args.print_config {
        return print_config_from_args(&args);
    }

    // Determine configuration source
    let config = load_configuration(&args, config_path)?;

    if config.sockets.is_empty() {
        bail!("No sockets configured. Use --socket option or define sockets in config file.");
    }

    info!(
        default_upstream = %config.upstream.display(),
        socket_count = config.sockets.len(),
        "Starting authsock-filter"
    );

    // Log configuration
    for (name, spec) in &config.sockets {
        let upstream_display = spec.upstream.as_ref().unwrap_or(&config.upstream);
        info!(
            name = %name,
            socket = %spec.path.display(),
            upstream = %upstream_display.display(),
            filters = ?spec.filters,
            "Configured socket"
        );
    }

    // Validate default upstream socket exists
    if !config.upstream.exists() {
        bail!(
            "Default upstream socket does not exist: {}",
            config.upstream.display()
        );
    }

    // Cache for upstream connections (to avoid creating duplicate Upstream instances)
    use std::collections::HashMap as UpstreamCache;
    let mut upstream_cache: UpstreamCache<PathBuf, Arc<Upstream>> = UpstreamCache::new();
    upstream_cache.insert(
        config.upstream.clone(),
        Arc::new(Upstream::new(config.upstream.to_string_lossy().to_string())),
    );

    // Start proxy servers for each socket
    let mut handles = Vec::new();
    let mut socket_paths = Vec::new();

    for spec in config.sockets.values() {
        // Determine upstream for this socket
        let upstream_path = spec.upstream.as_ref().unwrap_or(&config.upstream);

        // Validate socket-specific upstream if overridden
        if spec.upstream.is_some() && !upstream_path.exists() {
            bail!(
                "Upstream socket does not exist for {}: {}",
                spec.path.display(),
                upstream_path.display()
            );
        }

        // Get or create upstream connection manager
        let upstream = upstream_cache
            .entry(upstream_path.clone())
            .or_insert_with(|| Arc::new(Upstream::new(upstream_path.to_string_lossy().to_string())))
            .clone();

        // Parse filters
        let filter = FilterEvaluator::parse(&spec.filters).context(format!(
            "Failed to parse filters for socket {}",
            spec.path.display()
        ))?;

        // Ensure async filters are loaded (e.g., GitHub keys)
        filter.ensure_loaded().await.context(format!(
            "Failed to load filter data for socket {}",
            spec.path.display()
        ))?;

        let socket_path_str = spec.path.to_string_lossy().to_string();

        // Create proxy
        let proxy = Arc::new(
            Proxy::new_shared(upstream, Arc::new(filter)).with_socket_path(&socket_path_str),
        );

        // Remove existing socket if present
        if spec.path.exists() {
            debug!(path = %spec.path.display(), "Removing existing socket file");
            std::fs::remove_file(&spec.path).context(format!(
                "Failed to remove existing socket at {}",
                spec.path.display()
            ))?;
        }

        // Ensure parent directory exists
        if let Some(parent) = spec.path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)
                .context(format!("Failed to create directory {}", parent.display()))?;
        }

        // Bind listener
        let listener = UnixListener::bind(&spec.path)
            .context(format!("Failed to bind to socket {}", spec.path.display()))?;

        info!(
            path = %spec.path.display(),
            upstream = %upstream_path.display(),
            "Listening on socket"
        );

        socket_paths.push(spec.path.clone());

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

    // Clean up socket files
    for path in socket_paths {
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                debug!(path = %path.display(), error = %e, "Failed to remove socket file");
            } else {
                debug!(path = %path.display(), "Removed socket file");
            }
        }
    }

    info!("Shutdown complete");

    Ok(())
}

/// Load configuration from CLI args or config file
fn load_configuration(args: &RunArgs, config_path: Option<PathBuf>) -> Result<ExpandedConfig> {
    // If CLI arguments are provided, use them
    let cli_groups = args.parse_upstream_groups();
    if !cli_groups.is_empty() {
        // Convert CLI args to ExpandedConfig
        use crate::config::ExpandedSocketConfig;
        use std::collections::HashMap;

        let default_upstream = cli_groups[0].path.clone();
        let mut sockets = HashMap::new();

        for (idx, group) in cli_groups.iter().enumerate() {
            // If this group has a different upstream than the default, set it per-socket
            let socket_upstream = if group.path != default_upstream {
                Some(group.path.clone())
            } else {
                None
            };

            for (sock_idx, spec) in group.sockets.iter().enumerate() {
                let name = format!("socket_{}_{}", idx, sock_idx);
                sockets.insert(
                    name,
                    ExpandedSocketConfig {
                        path: spec.path.clone(),
                        upstream: socket_upstream.clone(),
                        filters: spec.filters.clone(),
                    },
                );
            }
        }

        return Ok(ExpandedConfig {
            upstream: default_upstream,
            sockets,
            github: crate::config::ExpandedGithubConfig {
                cache_ttl: std::time::Duration::from_secs(3600),
                timeout: std::time::Duration::from_secs(10),
            },
        });
    }

    // Try to load from config file
    let config_file_path = config_path
        .or_else(find_config_file)
        .context("No configuration found. Use --socket option or create a config file.")?;

    info!(path = %config_file_path.display(), "Loading configuration");

    let config_file = load_config(&config_file_path)?;
    config_file
        .config
        .expand_paths()
        .map_err(|e| anyhow::anyhow!("{}", e))
}

/// Print configuration as TOML from CLI arguments
fn print_config_from_args(args: &RunArgs) -> Result<()> {
    use std::collections::HashMap;

    let cli_groups = args.parse_upstream_groups();
    if cli_groups.is_empty() {
        bail!("No configuration to print. Use --upstream and --socket options.");
    }

    // Build Config structure
    let default_upstream = cli_groups[0].path.to_string_lossy().to_string();
    let mut sockets = HashMap::new();

    for group in &cli_groups {
        let group_upstream = group.path.to_string_lossy().to_string();
        // Set per-socket upstream if different from default
        let socket_upstream = if group_upstream != default_upstream {
            Some(group_upstream)
        } else {
            None
        };

        for spec in &group.sockets {
            // Generate a name from socket path
            let name = spec
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("socket")
                .to_string();

            // Handle duplicate names
            let final_name = if sockets.contains_key(&name) {
                let mut i = 2;
                loop {
                    let candidate = format!("{}-{}", name, i);
                    if !sockets.contains_key(&candidate) {
                        break candidate;
                    }
                    i += 1;
                }
            } else {
                name
            };

            sockets.insert(
                final_name,
                SocketConfig {
                    path: spec.path.to_string_lossy().to_string(),
                    upstream: socket_upstream.clone(),
                    filters: spec.filters.clone(),
                },
            );
        }
    }

    let config = Config {
        upstream: default_upstream,
        sockets,
        github: Default::default(),
    };

    let toml = toml::to_string_pretty(&config).context("Failed to serialize config")?;

    println!("# Generated configuration");
    println!("# Save to ~/.config/authsock-filter/config.toml");
    println!();
    print!("{}", toml);

    Ok(())
}
