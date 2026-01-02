//! Argument structures for CLI commands

use clap::Args;
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate, PathCompleter, ValueCompleter};
use clap_complete::Shell;
use std::ffi::OsStr;
use std::path::PathBuf;

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

    /// Socket definitions (repeatable)
    ///
    /// Format: /path/to/socket.sock:filter1:filter2...
    ///
    /// Examples:
    ///   --socket /tmp/filtered.sock:fingerprint:SHA256:xxx
    ///   --socket /tmp/github.sock:github:kawaz
    #[arg(long = "socket", value_name = "SPEC", add = ArgValueCompleter::new(socket_spec_completer))]
    pub sockets: Vec<String>,

    /// Foreground mode (don't daemonize) - always true for `run`
    #[arg(long, hide = true, default_value = "true")]
    pub foreground: bool,
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

    /// Socket definitions (repeatable)
    #[arg(long = "socket", value_name = "SPEC", add = ArgValueCompleter::new(socket_spec_completer))]
    pub sockets: Vec<String>,

    /// PID file path
    #[arg(long)]
    pub pid_file: Option<PathBuf>,
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

    /// Socket definitions for service
    #[arg(long = "socket", value_name = "SPEC", add = ArgValueCompleter::new(socket_spec_completer))]
    pub sockets: Vec<String>,
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

/// Filter types for socket spec completion
const FILTER_TYPES: &[(&str, &str)] = &[
    ("fingerprint:", "Filter by key fingerprint (SHA256:...)"),
    ("comment:", "Filter by key comment (glob/regex)"),
    ("github:", "Filter by GitHub username's keys"),
    ("type:", "Filter by key type (ed25519/rsa/ecdsa/dsa)"),
    ("pubkey:", "Filter by public key"),
    ("keyfile:", "Filter by keys in authorized_keys file"),
    ("-fingerprint:", "Exclude by fingerprint"),
    ("-comment:", "Exclude by comment"),
    ("-github:", "Exclude GitHub user's keys"),
    ("-type:", "Exclude by key type"),
    ("-pubkey:", "Exclude by public key"),
    ("-keyfile:", "Exclude keys in file"),
];

/// Custom completer for socket spec (-s option)
///
/// Completes:
/// - Path when no `:` present
/// - Filter types after `path:`
fn socket_spec_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current_str) = current.to_str() else {
        return vec![];
    };

    // Find the last `:` to determine context
    if let Some(colon_pos) = current_str.rfind(':') {
        let prefix = &current_str[..=colon_pos];
        let partial = &current_str[colon_pos + 1..];

        // After a colon, suggest filter types
        FILTER_TYPES
            .iter()
            .filter(|(name, _)| name.starts_with(partial))
            .map(|(name, help)| {
                CompletionCandidate::new(format!("{}{}", prefix, name)).help(Some((*help).into()))
            })
            .collect()
    } else {
        // No colon yet - complete as file path
        PathCompleter::any().complete(current)
    }
}
