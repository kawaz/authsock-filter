//! Run command - execute the proxy in the foreground

use anyhow::{Context, Result, bail};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixListener;
use tokio::signal;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::agent::{Proxy, Upstream};
use crate::cli::args::RunArgs;
use crate::config::{Config, ExpandedConfig, SocketConfig, find_config_file, load_config};
use crate::filter::FilterEvaluator;
use crate::utils::socket::{prepare_socket_path, set_socket_permissions};

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

        // Prepare socket path (remove existing with symlink protection, create parent dir)
        prepare_socket_path(&spec.path)
            .context(format!("Failed to prepare socket at {}", spec.path.display()))?;

        // Bind listener
        let listener = UnixListener::bind(&spec.path)
            .context(format!("Failed to bind to socket {}", spec.path.display()))?;

        // Set socket permissions to 0600 (owner read/write only)
        set_socket_permissions(&spec.path)
            .context(format!("Failed to set permissions on socket at {}", spec.path.display()))?;

        // Record inode for monitoring
        let inode = std::fs::metadata(&spec.path).ok().map(|m| m.ino());
        info!(
            path = %spec.path.display(),
            upstream = %upstream_path.display(),
            inode = ?inode,
            "Listening on socket"
        );

        socket_paths.push((spec.path.clone(), inode));

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

    // Create shutdown channel for inode monitor
    let (shutdown_tx, _) = watch::channel(false);

    // Spawn inode monitoring task
    let socket_paths_for_monitor = socket_paths.clone();
    let monitor_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            for (path, original_inode) in &socket_paths_for_monitor {
                let current_inode = std::fs::metadata(path).ok().map(|m| m.ino());
                match (original_inode, current_inode) {
                    (Some(orig), Some(curr)) if *orig != curr => {
                        warn!(
                            path = %path.display(),
                            original = orig,
                            current = curr,
                            "Socket inode changed, exiting"
                        );
                        return true; // Signal to exit
                    }
                    (Some(_), None) => {
                        warn!(path = %path.display(), "Socket file removed, exiting");
                        return true; // Signal to exit
                    }
                    _ => {}
                }
            }
        }
    });

    // Wait for shutdown signal or inode change
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal, stopping...");
        }
        result = monitor_handle => {
            if result.unwrap_or(false) {
                info!("Socket file changed, stopping...");
            }
        }
    }

    // Signal shutdown
    let _ = shutdown_tx.send(true);

    // Cancel all listener tasks
    for handle in handles {
        handle.abort();
    }

    // Clean up socket files
    for (path, _) in socket_paths {
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
        let mut sockets: HashMap<String, ExpandedSocketConfig> = HashMap::new();

        for group in cli_groups.iter() {
            // If this group has a different upstream than the default, set it per-socket
            let socket_upstream = if group.path != default_upstream {
                Some(group.path.clone())
            } else {
                None
            };

            for spec in group.sockets.iter() {
                // Find existing socket with same path (for OR conditions)
                let existing_name = sockets
                    .iter()
                    .find(|(_, cfg)| cfg.path == spec.path)
                    .map(|(name, _)| name.clone());

                if let Some(name) = existing_name {
                    // Same path: add filters as OR group
                    if !spec.filters.is_empty() {
                        sockets
                            .get_mut(&name)
                            .unwrap()
                            .filters
                            .push(spec.filters.clone());
                    }
                } else {
                    // New socket path
                    let name = spec
                        .path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("socket")
                        .to_string();

                    // Handle duplicate names (different paths with same filename)
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
                        ExpandedSocketConfig {
                            path: spec.path.clone(),
                            upstream: socket_upstream.clone(),
                            // CLI args are a single AND group
                            filters: if spec.filters.is_empty() {
                                vec![]
                            } else {
                                vec![spec.filters.clone()]
                            },
                        },
                    );
                }
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
    let mut sockets: HashMap<String, SocketConfig> = HashMap::new();

    for group in &cli_groups {
        let group_upstream = group.path.to_string_lossy().to_string();
        // Set per-socket upstream if different from default
        let socket_upstream = if group_upstream != default_upstream {
            Some(group_upstream)
        } else {
            None
        };

        for spec in &group.sockets {
            let socket_path = spec.path.to_string_lossy().to_string();

            // Find existing socket with same path (for OR conditions)
            let existing_name = sockets
                .iter()
                .find(|(_, cfg)| cfg.path == socket_path)
                .map(|(name, _)| name.clone());

            if let Some(name) = existing_name {
                // Same path: add filters as OR group
                if !spec.filters.is_empty() {
                    sockets
                        .get_mut(&name)
                        .unwrap()
                        .filters
                        .push(spec.filters.clone());
                }
            } else {
                // New socket path
                let name = spec
                    .path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("socket")
                    .to_string();

                // Handle duplicate names (different paths with same filename)
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
                        path: socket_path,
                        upstream: socket_upstream.clone(),
                        // CLI args are a single AND group
                        filters: if spec.filters.is_empty() {
                            vec![]
                        } else {
                            vec![spec.filters.clone()]
                        },
                    },
                );
            }
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
