//! Argument structures for CLI commands

use clap::Args;
use std::path::PathBuf;

/// Arguments for the `run` command
#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    /// Upstream SSH agent socket path
    ///
    /// Defaults to the value of SSH_AUTH_SOCK environment variable
    #[arg(short, long, env = "SSH_AUTH_SOCK")]
    pub upstream: Option<PathBuf>,

    /// Path to JSONL log file
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Socket definitions (repeatable)
    ///
    /// Format: /path/to/socket.sock:filter1:filter2...
    ///
    /// Examples:
    ///   -s /tmp/filtered.sock:fingerprint:SHA256:xxx
    ///   -s /tmp/github.sock:github:kawaz
    #[arg(short, long = "socket", value_name = "SPEC")]
    pub sockets: Vec<String>,

    /// Foreground mode (don't daemonize) - always true for `run`
    #[arg(long, hide = true, default_value = "true")]
    pub foreground: bool,
}

/// Arguments for the `start` command
#[derive(Args, Debug, Clone)]
pub struct StartArgs {
    /// Upstream SSH agent socket path
    #[arg(short, long, env = "SSH_AUTH_SOCK")]
    pub upstream: Option<PathBuf>,

    /// Path to JSONL log file
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Socket definitions (repeatable)
    #[arg(short, long = "socket", value_name = "SPEC")]
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
    #[arg(short, long)]
    pub force: bool,

    /// Timeout in seconds for graceful shutdown
    #[arg(short, long, default_value = "10")]
    pub timeout: u64,
}

/// Arguments for the `status` command
#[derive(Args, Debug, Clone)]
pub struct StatusArgs {
    /// PID file path
    #[arg(long)]
    pub pid_file: Option<PathBuf>,

    /// Output format
    #[arg(short, long, default_value = "text", value_parser = ["text", "json"])]
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
    #[arg(short, long, default_value = "toml", value_parser = ["toml", "json"])]
    pub format: String,
}

/// Arguments for the `upgrade` command
#[derive(Args, Debug, Clone)]
pub struct UpgradeArgs {
    /// Target version (default: latest)
    #[arg(long)]
    pub version: Option<String>,

    /// Force upgrade even if already at target version
    #[arg(short, long)]
    pub force: bool,

    /// Check for updates only, don't install
    #[arg(long)]
    pub check: bool,

    /// Skip confirmation prompt
    #[arg(short, long)]
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
    #[arg(short, long)]
    pub upstream: Option<PathBuf>,

    /// Socket definitions for service
    #[arg(short, long = "socket", value_name = "SPEC")]
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
