//! CLI module for authsock-filter
//!
//! This module provides the command-line interface using clap derive macros.

pub mod args;
pub mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use args::{
    CompletionArgs, ConfigArgs, RegisterArgs, RunArgs, StartArgs, StatusArgs, StopArgs,
    UnregisterArgs, UpgradeArgs,
};

/// SSH agent proxy with filtering and logging
#[derive(Parser, Debug)]
#[command(name = "authsock-filter")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
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

    /// Start the proxy as a background daemon
    Start(StartArgs),

    /// Stop the running daemon
    Stop(StopArgs),

    /// Show the status of the daemon
    Status(StatusArgs),

    /// Show or validate configuration
    Config(ConfigArgs),

    /// Show version information
    Version,

    /// Upgrade to the latest version from GitHub
    Upgrade(UpgradeArgs),

    /// Register as an OS service (launchd/systemd)
    Register(RegisterArgs),

    /// Unregister the OS service
    Unregister(UnregisterArgs),

    /// Generate shell completions
    Completion(CompletionArgs),
}
