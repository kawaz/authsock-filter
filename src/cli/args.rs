//! Argument structures for CLI commands

use clap::Args;
use clap_complete::Shell;
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use std::path::PathBuf;

/// Parsed socket configuration from CLI arguments
#[derive(Debug, Clone, Default)]
pub struct SocketSpec {
    pub path: PathBuf,
    pub filters: Vec<String>,
}

/// Upstream group containing an upstream path and its associated sockets
#[derive(Debug, Clone)]
pub struct UpstreamGroup {
    pub path: PathBuf,
    pub sockets: Vec<SocketSpec>,
}

/// Arguments for the `run` command
#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    /// Upstream SSH agent socket path
    ///
    /// Each --upstream starts a new group. Subsequent --socket definitions
    /// belong to that upstream until the next --upstream.
    ///
    /// Defaults to the value of SSH_AUTH_SOCK environment variable if not specified.
    #[arg(long, num_args = 1, action = clap::ArgAction::Append, add = ArgValueCompleter::new(upstream_completer))]
    pub upstream: Vec<PathBuf>,

    /// Socket definition with filters and options
    ///
    /// Format: --socket PATH [FILTERS...]
    ///
    /// Arguments after PATH until the next --socket are associated with this socket:
    ///   - Filters: type=value (e.g., comment=*@work*, github=kawaz, -type=dsa)
    ///
    /// Examples:
    ///   --socket /tmp/work.sock comment=*@work* type=ed25519
    ///   --socket /tmp/github.sock github=kawaz
    #[arg(long, num_args = 1.., value_name = "PATH [ARGS...]", allow_hyphen_values = true, add = ArgValueCompleter::new(socket_completer))]
    pub socket: Vec<String>,

    /// Print configuration as TOML and exit (useful for creating config file)
    #[arg(long)]
    pub print_config: bool,

    /// Foreground mode (don't daemonize) - always true for `run`
    #[arg(long, hide = true, default_value = "true")]
    pub foreground: bool,
}

impl RunArgs {
    /// Parse upstream groups from command line arguments
    ///
    /// Each --upstream starts a new group, and subsequent --socket definitions
    /// belong to that upstream until the next --upstream.
    pub fn parse_upstream_groups(&self) -> Vec<UpstreamGroup> {
        parse_upstream_groups_from_args()
    }
}

/// Arguments for the `config` command
#[derive(Args, Debug, Clone)]
pub struct ConfigArgs {
    /// Validate configuration only
    #[arg(long)]
    pub validate: bool,

    /// Show example configuration
    #[arg(long)]
    pub example: bool,

    /// Output format
    #[arg(long, default_value = "toml", value_parser = ["toml", "json"])]
    pub format: String,
}

/// Arguments for the `register` command
#[derive(Args, Debug, Clone)]
pub struct RegisterArgs {
    /// Service name
    #[arg(long, default_value = "authsock-filter")]
    pub name: String,

    /// Path to the executable for the service
    ///
    /// By default, uses the current executable path.
    /// If running from a version manager (mise, asdf, etc.),
    /// consider specifying a stable path like the shims directory.
    #[arg(long, value_name = "PATH")]
    pub executable: Option<PathBuf>,

    /// Allow registering with a version-managed path (may break after upgrade)
    #[arg(long)]
    pub allow_versioned_path: bool,
}

/// Arguments for the `unregister` command
#[derive(Args, Debug, Clone)]
pub struct UnregisterArgs {
    /// Service name
    #[arg(long, default_value = "authsock-filter")]
    pub name: String,
}

/// Arguments for the `completion` command
#[derive(Args, Debug, Clone)]
pub struct CompletionArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Parse upstream groups from command line arguments
///
/// Each --upstream starts a new group. Subsequent --socket definitions
/// belong to that upstream until the next --upstream.
///
/// If no --upstream is specified, uses SSH_AUTH_SOCK environment variable.
pub fn parse_upstream_groups_from_args() -> Vec<UpstreamGroup> {
    let args: Vec<String> = std::env::args().collect();
    let mut groups: Vec<UpstreamGroup> = Vec::new();
    let mut current_group: Option<UpstreamGroup> = None;
    let mut current_socket: Option<SocketSpec> = None;

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--upstream" || arg.starts_with("--upstream=") {
            // Save current socket to current group if any
            if let Some(spec) = current_socket.take()
                && let Some(ref mut group) = current_group
            {
                group.sockets.push(spec);
            }
            // Save current group if any
            if let Some(group) = current_group.take()
                && !group.sockets.is_empty()
            {
                groups.push(group);
            }

            // Get upstream path
            let path = if arg == "--upstream" {
                iter.next().map(|s| s.as_str())
            } else {
                arg.strip_prefix("--upstream=")
            };

            if let Some(path) = path {
                current_group = Some(UpstreamGroup {
                    path: expand_path(path),
                    sockets: Vec::new(),
                });
            }
        } else if arg == "--socket" || arg.starts_with("--socket=") {
            // Save previous socket if any
            if let Some(spec) = current_socket.take()
                && let Some(ref mut group) = current_group
            {
                group.sockets.push(spec);
            }

            // Get socket path
            let path = if arg == "--socket" {
                iter.next().map(|s| s.as_str())
            } else {
                arg.strip_prefix("--socket=")
            };

            if let Some(path) = path {
                // If no upstream yet, create default from SSH_AUTH_SOCK
                if current_group.is_none()
                    && let Ok(ssh_auth_sock) = std::env::var("SSH_AUTH_SOCK")
                {
                    current_group = Some(UpstreamGroup {
                        path: PathBuf::from(ssh_auth_sock),
                        sockets: Vec::new(),
                    });
                }

                current_socket = Some(SocketSpec {
                    path: expand_path(path),
                    filters: Vec::new(),
                });
            }
        } else if let Some(ref mut spec) = current_socket {
            // Arguments after --socket PATH belong to this socket
            // Skip known global options
            if arg.starts_with("--config")
                || arg.starts_with("--verbose")
                || arg.starts_with("--quiet")
                || arg.starts_with("--name")
                || arg.starts_with("--start")
                || arg.starts_with("--enable")
                || arg.starts_with("--purge")
                || arg == "-h"
                || arg == "--help"
                || arg == "-V"
                || arg == "--version"
            {
                // Skip global option and its value if needed
                if arg == "--config" || arg == "--name" {
                    iter.next(); // skip value
                }
                continue;
            }

            // Check if it's a filter (contains '=' and doesn't start with --)
            // or starts with - for negation filters
            if !arg.starts_with("--") {
                spec.filters.push(arg.clone());
            }
        }
    }

    // Save last socket to current group
    if let Some(spec) = current_socket
        && let Some(ref mut group) = current_group
    {
        group.sockets.push(spec);
    }

    // Save last group
    if let Some(group) = current_group
        && !group.sockets.is_empty()
    {
        groups.push(group);
    }

    groups
}

/// Expand path with ~ and environment variables
fn expand_path(path: &str) -> PathBuf {
    let expanded = shellexpand::full(path).unwrap_or(std::borrow::Cow::Borrowed(path));
    PathBuf::from(expanded.as_ref())
}

/// Completer for --upstream arguments (path completion)
fn upstream_completer(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    complete_path(&current)
}

/// Filter types for completion
const FILTER_TYPES: &[(&str, &str)] = &[
    ("fingerprint=", "Match by key fingerprint (SHA256:xxx)"),
    ("comment=", "Match by comment (glob or ~regex)"),
    ("github=", "Match keys from github.com/username.keys"),
    ("type=", "Match by key type (ed25519, rsa, ecdsa, dsa)"),
    ("pubkey=", "Match by full public key"),
    ("keyfile=", "Match keys from file"),
    ("-fingerprint=", "Exclude by fingerprint"),
    ("-comment=", "Exclude by comment"),
    ("-github=", "Exclude GitHub user keys"),
    ("-type=", "Exclude key type"),
    ("-pubkey=", "Exclude by public key"),
    ("-keyfile=", "Exclude keys from file"),
];

/// Key types for type= filter completion
const KEY_TYPES: &[&str] = &["ed25519", "rsa", "ecdsa", "dsa"];

/// Completer for --socket arguments
fn socket_completer(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();

    // Check if this looks like a filter (contains = or starts with -)
    if current.contains('=') {
        // Already has type=, complete the value if it's type=
        if let Some(prefix) = current.strip_prefix("type=") {
            return KEY_TYPES
                .iter()
                .filter(|t| t.starts_with(prefix))
                .map(|t| {
                    CompletionCandidate::new(format!("type={}", t))
                        .help(Some(format!("{} keys", t).into()))
                })
                .collect();
        }
        if let Some(prefix) = current.strip_prefix("-type=") {
            return KEY_TYPES
                .iter()
                .filter(|t| t.starts_with(prefix))
                .map(|t| {
                    CompletionCandidate::new(format!("-type={}", t))
                        .help(Some(format!("Exclude {} keys", t).into()))
                })
                .collect();
        }
        // keyfile= and -keyfile= need path completion
        if let Some(path_prefix) = current.strip_prefix("keyfile=") {
            return complete_path(path_prefix)
                .into_iter()
                .map(|c| {
                    CompletionCandidate::new(format!("keyfile={}", c.get_value().to_string_lossy()))
                })
                .collect();
        }
        if let Some(path_prefix) = current.strip_prefix("-keyfile=") {
            return complete_path(path_prefix)
                .into_iter()
                .map(|c| {
                    CompletionCandidate::new(format!(
                        "-keyfile={}",
                        c.get_value().to_string_lossy()
                    ))
                })
                .collect();
        }
        // Other filter types - no value completion
        return vec![];
    }

    // Check if it starts with - (negation filter prefix)
    if current.starts_with('-') && !current.starts_with("--") {
        // Complete negation filters
        return FILTER_TYPES
            .iter()
            .filter(|(name, _)| name.starts_with('-') && name.starts_with(current.as_ref()))
            .map(|(name, help)| CompletionCandidate::new(*name).help(Some((*help).into())))
            .collect();
    }

    // Check if it looks like a path (starts with / or ~ or .)
    if current.starts_with('/') || current.starts_with('~') || current.starts_with('.') {
        // Path completion
        return complete_path(&current);
    }

    // Empty or partial input - show both paths and filters
    if current.is_empty() {
        // Show filter types as primary suggestions
        return FILTER_TYPES
            .iter()
            .map(|(name, help)| CompletionCandidate::new(*name).help(Some((*help).into())))
            .collect();
    }

    // Partial filter type
    let mut candidates: Vec<CompletionCandidate> = FILTER_TYPES
        .iter()
        .filter(|(name, _)| name.starts_with(current.as_ref()))
        .map(|(name, help)| CompletionCandidate::new(*name).help(Some((*help).into())))
        .collect();

    // Also try path completion if no filter matches
    if candidates.is_empty() {
        candidates = complete_path(&current);
    }

    candidates
}

/// Complete file paths
fn complete_path(current: &str) -> Vec<CompletionCandidate> {
    use std::fs;
    use std::path::Path;

    let path = if current.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            let rest = current.strip_prefix('~').unwrap_or("");
            let rest = rest.strip_prefix('/').unwrap_or(rest);
            home.join(rest)
        } else {
            PathBuf::from(current)
        }
    } else {
        PathBuf::from(current)
    };

    let (dir, prefix) = if path.is_dir() {
        (path.clone(), "")
    } else {
        (
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
        )
    };

    let mut candidates = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(prefix) {
                let full_path = entry.path();
                let display = if current.starts_with('~') {
                    if let Some(home) = dirs::home_dir() {
                        if let Ok(rel) = full_path.strip_prefix(&home) {
                            format!("~/{}", rel.display())
                        } else {
                            full_path.display().to_string()
                        }
                    } else {
                        full_path.display().to_string()
                    }
                } else {
                    full_path.display().to_string()
                };
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let display = if is_dir {
                    format!("{}/", display)
                } else {
                    display
                };
                candidates.push(CompletionCandidate::new(display));
            }
        }
    }
    candidates
}
