//! authsock-filter - SSH agent proxy with key filtering

use clap::{CommandFactory, Parser};
use clap_complete::env::CompleteEnv;
use tracing::error;
use tracing_subscriber::EnvFilter;

use authsock_filter::cli::exit_code::ExitCode;
use authsock_filter::cli::{Cli, Commands, ServiceCommand};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Handle dynamic shell completion if COMPLETE env var is set
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    // Handle --version flag before logging initialization
    if cli.version {
        authsock_filter::cli::commands::version::print_version(cli.verbose);
        return ExitCode::Success.into();
    }

    // Initialize logging
    init_logging(cli.verbose, cli.quiet);

    let result = run(cli).await;

    match result {
        Ok(()) => ExitCode::Success.into(),
        Err((code, err)) => {
            error!("{err:#}");
            code.into()
        }
    }
}

async fn run(cli: Cli) -> Result<(), (ExitCode, anyhow::Error)> {
    let Some(command) = cli.command else {
        // No subcommand provided - show help
        Cli::command().print_help().ok();
        return Ok(());
    };

    match command {
        Commands::Run(args) => authsock_filter::cli::commands::run::execute(args, cli.config)
            .await
            .map_err(|e| (classify_error(&e), e))?,
        Commands::Config { command } => {
            authsock_filter::cli::commands::config::execute(command, cli.config)
                .await
                .map_err(|e| (ExitCode::ConfigError, e))?
        }
        Commands::Service { command } => match command {
            ServiceCommand::Register(args) => {
                authsock_filter::cli::commands::service::register(args, cli.config)
                    .await
                    .map_err(|e| (ExitCode::GeneralError, e))?
            }
            ServiceCommand::Unregister(args) => {
                authsock_filter::cli::commands::service::unregister(args)
                    .await
                    .map_err(|e| (ExitCode::GeneralError, e))?
            }
            ServiceCommand::Reload(args) => authsock_filter::cli::commands::service::reload(args)
                .await
                .map_err(|e| (ExitCode::GeneralError, e))?,
            ServiceCommand::Status(args) => authsock_filter::cli::commands::service::status(args)
                .await
                .map_err(|e| (ExitCode::GeneralError, e))?,
        },
        Commands::Completion(args) => authsock_filter::cli::commands::completion::execute(args)
            .await
            .map_err(|e| (ExitCode::GeneralError, e))?,
        Commands::Version => {
            authsock_filter::cli::commands::version::print_version(cli.verbose);
        }
    }

    Ok(())
}

/// Classify an error to determine the appropriate exit code
fn classify_error(err: &anyhow::Error) -> ExitCode {
    let err_str = format!("{err:#}").to_lowercase();

    if err_str.contains("config") || err_str.contains("configuration") {
        ExitCode::ConfigError
    } else if err_str.contains("upstream") || err_str.contains("ssh_auth_sock") {
        ExitCode::UpstreamError
    } else if err_str.contains("socket") || err_str.contains("bind") || err_str.contains("listen") {
        ExitCode::SocketError
    } else {
        ExitCode::GeneralError
    }
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
