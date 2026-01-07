//! CLI module for authsock-filter
//!
//! This module provides the command-line interface using clap derive macros.

pub mod args;
pub mod commands;
pub mod exit_code;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use args::{CompletionArgs, RegisterArgs, RunArgs, UnregisterArgs};

/// SSH agent proxy with key filtering
#[derive(Parser, Debug)]
#[command(name = "authsock-filter")]
#[command(author, about, long_about = None)]
#[command(disable_help_flag = true, disable_version_flag = true)]
pub struct Cli {
    /// Print help
    #[arg(long, action = clap::ArgAction::Help, global = true)]
    help: Option<bool>,

    /// Print version (use -v/--verbose with --version for detailed info)
    #[arg(short = 'V', long)]
    pub version: bool,

    /// Configuration file path
    #[arg(long, global = true, env = "AUTHSOCK_FILTER_CONFIG")]
    pub config: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Suppress non-essential output
    #[arg(long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the proxy in the foreground
    Run(RunArgs),

    /// Manage configuration file
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },

    /// Manage OS service (launchd/systemd)
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },

    /// Generate shell completions
    Completion(CompletionArgs),

    /// Print version information (hidden alias for -V/--version)
    #[command(hide = true)]
    Version,
}

/// Config management commands
#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommand {
    /// Show configuration content (default)
    Show,

    /// Open configuration in editor
    Edit,

    /// Print configuration file path
    Path,

    /// Output as CLI command arguments
    Command,
}

/// Service management commands
#[derive(Subcommand, Debug, Clone)]
pub enum ServiceCommand {
    /// Register and start as an OS service
    Register(RegisterArgs),

    /// Stop and unregister the OS service
    Unregister(UnregisterArgs),

    /// Reload configuration (restart service)
    Reload(UnregisterArgs),

    /// Show service status
    Status(UnregisterArgs),
}
