//! Version command - show version information

use anyhow::Result;

/// Execute the version command
pub async fn execute() -> Result<()> {
    println!("{} {}", crate::NAME, crate::VERSION);
    println!();
    println!("Features:");
    println!("  - SSH agent proxy with filtering");
    println!("  - JSONL logging support");
    println!("  - Multiple filter types (fingerprint, github, comment, keytype)");
    println!("  - Daemon mode with OS service integration");
    println!();
    println!("Build info:");
    println!("  Target:    {}", std::env::consts::ARCH);
    println!("  OS:        {}", std::env::consts::OS);
    println!("  Rust:      {}", env!("CARGO_PKG_RUST_VERSION", "unknown"));
    println!();
    println!("Repository: https://github.com/kawaz/authsock-filter");
    println!("License:    MIT");

    Ok(())
}
