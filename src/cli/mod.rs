//! CLI module for authsock-filter
//!
//! This module provides the command-line interface using clap derive macros.

pub mod args;
pub mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use args::{CompletionArgs, ConfigArgs, RegisterArgs, RunArgs, UnregisterArgs, UpgradeArgs};

/// SSH agent proxy with filtering and logging
#[derive(Parser, Debug)]
#[command(name = "authsock-filter")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(disable_help_flag = true, disable_version_flag = true)]
pub struct Cli {
    /// Print help
    #[arg(long, action = clap::ArgAction::Help, global = true)]
    help: Option<bool>,

    /// Print version
    #[arg(long, action = clap::ArgAction::Version, global = true)]
    version: Option<bool>,
    /// Configuration file path
    #[arg(long, global = true, env = "AUTHSOCK_FILTER_CONFIG")]
    pub config: Option<PathBuf>,

    /// Enable verbose output
    #[arg(long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Suppress non-essential output
    #[arg(long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the proxy in the foreground
    Run(RunArgs),

    /// Show or validate configuration
    Config(ConfigArgs),

    /// Show version information
    Version,

    /// Upgrade to the latest version from GitHub
    Upgrade(UpgradeArgs),

    /// Manage OS service (launchd/systemd)
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },

    /// Generate shell completions
    Completion(CompletionArgs),
}

/// Service management commands
#[derive(Subcommand, Debug, Clone)]
pub enum ServiceCommand {
    /// Register as an OS service
    Register(RegisterArgs),

    /// Unregister the OS service
    Unregister(UnregisterArgs),

    /// Show service status
    Status,

    /// Start the registered service
    Start,

    /// Stop the registered service
    Stop,

    /// Enable auto-start at login/boot
    Enable,

    /// Disable auto-start at login/boot
    Disable,
}
