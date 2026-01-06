//! Version command - show version information

/// Print version information
///
/// If verbose is false, prints a single line with name and version.
/// If verbose is true, prints detailed build and feature information.
pub fn print_version(verbose: bool) {
    println!("{} {}", crate::NAME, crate::VERSION);

    if verbose {
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
        println!("  Rust:      {}", env!("RUSTC_VERSION"));
        if let Ok(exe) = std::env::current_exe() {
            println!("  Executable: {}", exe.display());
        }
        println!();
        println!("Repository: https://github.com/kawaz/authsock-filter");
        println!("License:    MIT");
    }
}
