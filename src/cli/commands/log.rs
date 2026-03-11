//! Log viewing command

use anyhow::{Context, Result, bail};

use crate::cli::args::LogArgs;

/// Parse a duration string like "5m", "1h30m", "2d", "500ms"
///
/// Supported units: d (days), h (hours), m (minutes), s (seconds), ms (milliseconds)
/// A unit suffix is always required (e.g., "30s" not "30").
fn parse_duration(s: &str) -> Result<std::time::Duration> {
    let mut total_ms: u64 = 0;
    let mut chars = s.chars().peekable();
    let mut has_any = false;

    while chars.peek().is_some() {
        // Collect digits
        let mut num_str = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                num_str.push(c);
                chars.next();
            } else {
                break;
            }
        }

        if num_str.is_empty() {
            let c = chars.next().unwrap();
            bail!("Invalid duration format: unexpected '{c}'");
        }

        let n: u64 = num_str.parse().context("Invalid duration number")?;

        // Collect unit (multi-char units like "ms")
        let mut unit = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() {
                unit.push(c);
                chars.next();
            } else {
                break;
            }
        }

        match unit.as_str() {
            "d" => total_ms += n * 86_400_000,
            "h" => total_ms += n * 3_600_000,
            "m" => total_ms += n * 60_000,
            "s" => total_ms += n * 1_000,
            "ms" => total_ms += n,
            "" => bail!("Missing unit after '{n}' (expected d, h, m, s, or ms)"),
            _ => bail!("Invalid duration unit '{unit}' (expected d, h, m, s, or ms)"),
        }
        has_any = true;
    }

    if !has_any {
        bail!("Empty duration");
    }

    if total_ms == 0 {
        bail!("Duration must be greater than 0");
    }

    Ok(std::time::Duration::from_millis(total_ms))
}

/// Format a Command for display (shell-quoted)
fn format_command(cmd: &std::process::Command) -> String {
    let parts: Vec<String> = std::iter::once(cmd.get_program().to_string_lossy().into_owned())
        .chain(cmd.get_args().map(|a| a.to_string_lossy().into_owned()))
        .collect();
    shlex::try_join(parts.iter().map(|s| s.as_str())).unwrap_or_else(|_| parts.join(" "))
}

/// Run a command, printing it to stderr first
fn run_command(mut cmd: std::process::Command) -> Result<()> {
    eprintln!("+ {}", format_command(&cmd));
    cmd.status().context(format!(
        "Failed to execute '{}'",
        cmd.get_program().to_string_lossy()
    ))?;
    Ok(())
}

pub async fn execute(args: LogArgs) -> Result<()> {
    // No arguments → show help
    if args.since.is_none() && !args.follow {
        use clap::CommandFactory;
        let mut cmd = crate::cli::Cli::command();
        cmd.build();
        cmd.find_subcommand_mut("log").unwrap().print_help().ok();
        return Ok(());
    }
    run_log_viewer(&args)
}

// ============================================================================
// macOS: unified log (log stream / log show)
// ============================================================================

#[cfg(target_os = "macos")]
fn run_log_viewer(args: &LogArgs) -> Result<()> {
    let output_log = dirs::home_dir()
        .context("Failed to get home directory")?
        .join("Library/Logs/authsock-filter/output.log");

    // Show historical logs
    if let Some(ref since) = args.since {
        let secs = parse_duration(since)?.as_secs().max(1);
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - secs;

        // unified log (launchd management logs)
        let predicate = "process == \"authsock-filter\" OR eventMessage CONTAINS \"com.github.kawaz.authsock-filter\"";
        let mut cmd = std::process::Command::new("log");
        cmd.args([
            "show",
            "--predicate",
            predicate,
            "--start",
            &format!("@{since_epoch}"),
            "--debug",
            "--info",
            "--style",
            "compact",
        ]);
        run_command(cmd)?;

        // stderr log file (process output: tracing logs)
        if output_log.exists() {
            eprintln!();
            let mut cmd = std::process::Command::new("tail");
            cmd.args(["-100", &output_log.display().to_string()]);
            run_command(cmd)?;
        }
    }

    // Follow mode: stream new entries
    if args.follow {
        if output_log.exists() {
            let mut cmd = std::process::Command::new("tail");
            cmd.args(["-f", &output_log.display().to_string()]);
            run_command(cmd)?;
        } else {
            // Fall back to unified log stream
            let predicate = "process == \"authsock-filter\" OR eventMessage CONTAINS \"com.github.kawaz.authsock-filter\"";
            let mut cmd = std::process::Command::new("log");
            cmd.args([
                "stream",
                "--predicate",
                predicate,
                "--level",
                "debug",
                "--style",
                "compact",
            ]);
            run_command(cmd)?;
        }
    }

    Ok(())
}

// ============================================================================
// Linux: journalctl
// ============================================================================

#[cfg(target_os = "linux")]
fn run_log_viewer(args: &LogArgs) -> Result<()> {
    let mut cmd = std::process::Command::new("journalctl");
    cmd.args(["--user", "-u", "authsock-filter", "--no-pager"]);

    if args.follow {
        cmd.arg("-f");
    }

    if let Some(ref since) = args.since {
        let secs = parse_duration(since)?.as_secs();
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - secs;
        cmd.args(["--since", &format!("@{since_epoch}")]);
    }

    run_command(cmd)
}

// ============================================================================
// Unsupported platforms
// ============================================================================

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn run_log_viewer(_args: &LogArgs) -> Result<()> {
    bail!("Log viewing is not supported on this platform")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap().as_millis(), 30_000);
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("5m").unwrap().as_secs(), 300);
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h").unwrap().as_secs(), 3600);
    }

    #[test]
    fn test_parse_duration_days() {
        assert_eq!(parse_duration("2d").unwrap().as_secs(), 172_800);
    }

    #[test]
    fn test_parse_duration_milliseconds() {
        assert_eq!(parse_duration("500ms").unwrap().as_millis(), 500);
        assert_eq!(parse_duration("1s500ms").unwrap().as_millis(), 1_500);
    }

    #[test]
    fn test_parse_duration_combined() {
        assert_eq!(parse_duration("1m30s").unwrap().as_secs(), 90);
        assert_eq!(parse_duration("1h30m").unwrap().as_secs(), 5400);
        assert_eq!(parse_duration("1h30m15s").unwrap().as_secs(), 5415);
        assert_eq!(parse_duration("1d12h").unwrap().as_secs(), 129_600);
    }

    #[test]
    fn test_parse_duration_bare_number_is_error() {
        assert!(parse_duration("30").is_err());
        assert!(parse_duration("0").is_err());
    }

    #[test]
    fn test_parse_duration_zero_is_error() {
        assert!(parse_duration("0s").is_err());
        assert!(parse_duration("0m").is_err());
        assert!(parse_duration("0ms").is_err());
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("5x").is_err());
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn test_format_command_simple() {
        let cmd = std::process::Command::new("log");
        assert_eq!(format_command(&cmd), "log");
    }

    #[test]
    fn test_format_command_with_args() {
        let mut cmd = std::process::Command::new("log");
        cmd.args(["show", "--predicate", "process == \"test\""]);
        assert_eq!(
            format_command(&cmd),
            r#"log show --predicate 'process == "test"'"#
        );
    }
}
