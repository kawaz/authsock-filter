//! authsock-filter - SSH agent proxy with filtering and logging

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::env::CompleteEnv;

use authsock_filter::cli::{Cli, Commands, ServiceCommand};

#[tokio::main]
async fn main() -> Result<()> {
    // Handle dynamic shell completion if COMPLETE env var is set
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    // Initialize logging
    let _log_guard = authsock_filter::logging::init(cli.verbose, cli.quiet);

    match cli.command {
        Commands::Run(args) => authsock_filter::cli::commands::run::execute(args).await?,
        Commands::Config(args) => authsock_filter::cli::commands::config::execute(args).await?,
        Commands::Version => authsock_filter::cli::commands::version::execute().await?,
        Commands::Upgrade(args) => authsock_filter::cli::commands::upgrade::execute(args).await?,
        Commands::Service { command } => match command {
            ServiceCommand::Register(args) => {
                authsock_filter::cli::commands::service::register(args).await?
            }
            ServiceCommand::Unregister(args) => {
                authsock_filter::cli::commands::service::unregister(args).await?
            }
            ServiceCommand::Start => authsock_filter::cli::commands::service::start().await?,
            ServiceCommand::Stop => authsock_filter::cli::commands::service::stop().await?,
            ServiceCommand::Status => authsock_filter::cli::commands::service::status().await?,
            ServiceCommand::Enable => authsock_filter::cli::commands::service::enable().await?,
            ServiceCommand::Disable => authsock_filter::cli::commands::service::disable().await?,
        },
        Commands::Completion(args) => {
            authsock_filter::cli::commands::completion::execute(args).await?
        }
    }

    Ok(())
}
