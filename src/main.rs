//! authsock-filter - SSH agent proxy with key filtering

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::env::CompleteEnv;
use tracing_subscriber::EnvFilter;

use authsock_filter::cli::{Cli, Commands, ServiceCommand};

#[tokio::main]
async fn main() -> Result<()> {
    // Handle dynamic shell completion if COMPLETE env var is set
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    // Initialize logging
    init_logging(cli.verbose, cli.quiet);

    match cli.command {
        Commands::Run(args) => {
            authsock_filter::cli::commands::run::execute(args, cli.config).await?
        }
        Commands::Config(args) => authsock_filter::cli::commands::config::execute(args).await?,
        Commands::Version => authsock_filter::cli::commands::version::execute().await?,
        Commands::Service { command } => match command {
            ServiceCommand::Register(args) => {
                authsock_filter::cli::commands::service::register(args, cli.config).await?
            }
            ServiceCommand::Unregister(args) => {
                authsock_filter::cli::commands::service::unregister(args).await?
            }
            ServiceCommand::Reload(args) => {
                authsock_filter::cli::commands::service::reload(args).await?
            }
        },
        Commands::Completion(args) => {
            authsock_filter::cli::commands::completion::execute(args).await?
        }
    }

    Ok(())
}

/// Initialize logging with tracing-subscriber
fn init_logging(verbose: bool, quiet: bool) {
    let filter = if verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else if quiet {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("error"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
