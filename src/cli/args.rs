//! Argument structures for CLI commands

use clap::Args;
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use clap_complete::Shell;
use std::path::PathBuf;

/// Parsed socket configuration from CLI arguments
#[derive(Debug, Clone, Default)]
pub struct SocketSpec {
    pub path: PathBuf,
    pub filters: Vec<String>,
}

/// Arguments for the `run` command
#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    /// Upstream SSH agent socket path
    ///
    /// Defaults to the value of SSH_AUTH_SOCK environment variable
    #[arg(long, env = "SSH_AUTH_SOCK")]
    pub upstream: Option<PathBuf>,

    /// Path to JSONL log file
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Socket definition with filters and options
    ///
    /// Format: --socket PATH [FILTERS...] [OPTIONS...]
    ///
    /// Arguments after PATH until the next --socket are associated with this socket:
    ///   - Filters: type=value (e.g., comment=*@work*, github=kawaz, -type=dsa)
    ///   - Options: --logging true, --mode 0666, etc.
    ///
    /// Examples:
    ///   --socket /tmp/work.sock comment=*@work* type=ed25519
    ///   --socket /tmp/github.sock github=kawaz --logging true
    #[arg(long, num_args = 1.., value_name = "PATH [ARGS...]", add = ArgValueCompleter::new(socket_completer))]
    pub socket: Vec<String>,

    /// Foreground mode (don't daemonize) - always true for `run`
    #[arg(long, hide = true, default_value = "true")]
    pub foreground: bool,
}

impl RunArgs {
    /// Parse socket and filter arguments into SocketSpecs
    /// Filters are associated with the preceding socket
    pub fn parse_socket_specs(&self) -> Vec<SocketSpec> {
        parse_socket_specs_from_args()
    }
}

/// Arguments for the `start` command
#[derive(Args, Debug, Clone)]
pub struct StartArgs {
    /// Upstream SSH agent socket path
    #[arg(long, env = "SSH_AUTH_SOCK")]
    pub upstream: Option<PathBuf>,

    /// Path to JSONL log file
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Socket definition with filters and options
    #[arg(long, num_args = 1.., value_name = "PATH [ARGS...]", add = ArgValueCompleter::new(socket_completer))]
    pub socket: Vec<String>,

    /// PID file path
    #[arg(long)]
    pub pid_file: Option<PathBuf>,
}

impl StartArgs {
    /// Parse socket and filter arguments into SocketSpecs
    pub fn parse_socket_specs(&self) -> Vec<SocketSpec> {
        parse_socket_specs_from_args()
    }
}

/// Arguments for the `stop` command
#[derive(Args, Debug, Clone)]
pub struct StopArgs {
    /// PID file path
    #[arg(long)]
    pub pid_file: Option<PathBuf>,

    /// Force stop without waiting
    #[arg(long)]
    pub force: bool,

    /// Timeout in seconds for graceful shutdown
    #[arg(long, default_value = "10")]
    pub timeout: u64,
}

/// Arguments for the `status` command
#[derive(Args, Debug, Clone)]
pub struct StatusArgs {
    /// PID file path
    #[arg(long)]
    pub pid_file: Option<PathBuf>,

    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json"])]
    pub format: String,
}

/// Arguments for the `config` command
#[derive(Args, Debug, Clone)]
pub struct ConfigArgs {
    /// Validate configuration only
    #[arg(long)]
    pub validate: bool,

    /// Show default configuration
    #[arg(long)]
    pub show_default: bool,

    /// Output format
    #[arg(long, default_value = "toml", value_parser = ["toml", "json"])]
    pub format: String,
}

/// Arguments for the `upgrade` command
#[derive(Args, Debug, Clone)]
pub struct UpgradeArgs {
    /// Force upgrade even if already at target version
    #[arg(long)]
    pub force: bool,

    /// Check for updates only, don't install
    #[arg(long)]
    pub check: bool,

    /// Skip confirmation prompt
    #[arg(long)]
    pub yes: bool,
}

/// Arguments for the `register` command
#[derive(Args, Debug, Clone)]
pub struct RegisterArgs {
    /// Service name
    #[arg(long, default_value = "authsock-filter")]
    pub name: String,

    /// Start service immediately after registration
    #[arg(long)]
    pub start: bool,

    /// Enable service to start at login/boot
    #[arg(long, default_value = "true")]
    pub enable: bool,

    /// Upstream SSH agent socket path for service
    #[arg(long)]
    pub upstream: Option<PathBuf>,

    /// Socket definition with filters and options
    #[arg(long, num_args = 1.., value_name = "PATH [ARGS...]", add = ArgValueCompleter::new(socket_completer))]
    pub socket: Vec<String>,
}

impl RegisterArgs {
    /// Parse socket and filter arguments into SocketSpecs
    pub fn parse_socket_specs(&self) -> Vec<SocketSpec> {
        parse_socket_specs_from_args()
    }
}

/// Arguments for the `unregister` command
#[derive(Args, Debug, Clone)]
pub struct UnregisterArgs {
    /// Service name
    #[arg(long, default_value = "authsock-filter")]
    pub name: String,

    /// Remove configuration files as well
    #[arg(long)]
    pub purge: bool,
}

/// Arguments for the `completion` command
#[derive(Args, Debug, Clone)]
pub struct CompletionArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Parse socket specs from command line arguments
///
/// New format: --socket PATH [FILTERS...] [OPTIONS...]
/// Arguments after PATH until the next --socket belong to this socket
pub fn parse_socket_specs_from_args() -> Vec<SocketSpec> {
    let args: Vec<String> = std::env::args().collect();
    let mut specs: Vec<SocketSpec> = Vec::new();
    let mut current_socket: Option<SocketSpec> = None;

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--socket" || arg.starts_with("--socket=") {
            // Save previous socket if any
            if let Some(spec) = current_socket.take() {
                specs.push(spec);
            }

            // Get socket path
            let path = if arg == "--socket" {
                iter.next().map(|s| s.as_str())
            } else {
                arg.strip_prefix("--socket=")
            };

            if let Some(path) = path {
                current_socket = Some(SocketSpec {
                    path: PathBuf::from(path),
                    filters: Vec::new(),
                });
            }
        } else if let Some(ref mut spec) = current_socket {
            // Arguments after --socket PATH belong to this socket
            // Skip known global options
            if arg.starts_with("--upstream")
                || arg.starts_with("--log")
                || arg.starts_with("--config")
                || arg.starts_with("--verbose")
                || arg.starts_with("--quiet")
                || arg.starts_with("--pid-file")
                || arg == "-h"
                || arg == "--help"
                || arg == "-V"
                || arg == "--version"
            {
                // Skip global option and its value if needed
                if arg == "--upstream" || arg == "--log" || arg == "--config" || arg == "--pid-file"
                {
                    iter.next(); // skip value
                }
                continue;
            }

            // Check if it's a filter (contains '=' and doesn't start with --)
            // or starts with - for negation filters
            if !arg.starts_with("--") {
                spec.filters.push(arg.clone());
            }
            // TODO: Handle socket-specific options like --logging, --mode
        }
    }

    // Save last socket
    if let Some(spec) = current_socket {
        specs.push(spec);
    }

    specs
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
        // Other filter types - no value completion yet
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
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(""),
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
