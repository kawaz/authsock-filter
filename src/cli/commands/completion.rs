//! Completion command implementation

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;
use std::io;

use crate::cli::args::CompletionArgs;
use crate::cli::Cli;

/// Execute the completion command
pub async fn execute(args: CompletionArgs) -> Result<()> {
    let mut cmd = Cli::command();
    generate(args.shell, &mut cmd, "authsock-filter", &mut io::stdout());
    Ok(())
}
