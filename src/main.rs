//! authsock-filter - SSH agent proxy with filtering and logging

use anyhow::Result;
use clap::Parser;

use authsock_filter::cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let _log_guard = authsock_filter::logging::init(cli.verbose, cli.quiet);

    match cli.command {
        Commands::Run(args) => authsock_filter::cli::commands::run::execute(args).await?,
        Commands::Start(args) => authsock_filter::cli::commands::start::execute(args).await?,
        Commands::Stop(args) => authsock_filter::cli::commands::stop::execute(args).await?,
        Commands::Status(args) => authsock_filter::cli::commands::status::execute(args).await?,
        Commands::Config(args) => authsock_filter::cli::commands::config::execute(args).await?,
        Commands::Version => authsock_filter::cli::commands::version::execute().await?,
        Commands::Upgrade(args) => authsock_filter::cli::commands::upgrade::execute(args).await?,
        Commands::Register(args) => authsock_filter::cli::commands::register::execute(args).await?,
        Commands::Unregister(args) => {
            authsock_filter::cli::commands::unregister::execute(args).await?
        }
        Commands::Completion(args) => {
            authsock_filter::cli::commands::completion::execute(args).await?
        }
    }

    Ok(())
}
