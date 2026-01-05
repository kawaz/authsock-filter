//! CLI module for authsock-filter
//!
//! This module provides the command-line interface using clap derive macros.

pub mod args;
pub mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use args::{CompletionArgs, ConfigArgs, RegisterArgs, RunArgs, UnregisterArgs};

/// SSH agent proxy with key filtering
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
    /// Register and start as an OS service (常駐化)
    Register(RegisterArgs),

    /// Stop and unregister the OS service (常駐化解除)
    Unregister(UnregisterArgs),

    /// Reload configuration (restart service)
    Reload(UnregisterArgs),
}
