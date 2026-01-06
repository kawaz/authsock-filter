//! Config command - manage configuration file

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

use crate::cli::ConfigCommand;
use crate::config::{config_search_paths, find_config_file, load_config};

/// Default configuration template
fn default_config() -> &'static str {
    r#"# authsock-filter configuration
# See https://github.com/kawaz/authsock-filter for documentation

upstream = "$SSH_AUTH_SOCK"

[sockets.default]
path = "/tmp/authsock-filter/default.sock"
filters = []
"#
}

/// Execute the config command
pub async fn execute(
    command: Option<ConfigCommand>,
    config_override: Option<PathBuf>,
) -> Result<()> {
    let command = command.unwrap_or(ConfigCommand::Show);

    match command {
        ConfigCommand::Show => show(config_override).await,
        ConfigCommand::Edit => edit(config_override).await,
        ConfigCommand::Path => path(config_override).await,
        ConfigCommand::Command => to_command(config_override).await,
    }
}

/// Show configuration content
async fn show(config_override: Option<PathBuf>) -> Result<()> {
    let config_path = config_override.or_else(find_config_file);

    if let Some(path) = config_path {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        print!("{}", content);
    } else {
        eprintln!("No configuration file found.");
        eprintln!("Create one with: authsock-filter config edit");
    }

    Ok(())
}

/// Open configuration in editor
async fn edit(config_override: Option<PathBuf>) -> Result<()> {
    let config_path = config_override.or_else(find_config_file);

    let path = match config_path {
        Some(p) => p,
        None => {
            // Create default config at first search path
            let default_path = config_search_paths()
                .first()
                .map(|cp| cp.path.clone())
                .context("No config search paths available")?;

            // Create parent directory if needed
            if let Some(parent) = default_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }

            // Write default config
            std::fs::write(&default_path, default_config()).with_context(|| {
                format!("Failed to create config file: {}", default_path.display())
            })?;

            eprintln!("Created: {}", default_path.display());
            default_path
        }
    };

    // Get editor from EDITOR env var or use platform default
    let editor = std::env::var("EDITOR").ok();

    #[cfg(target_os = "macos")]
    let (cmd, args) = match editor {
        Some(e) => (e, vec![path.display().to_string()]),
        None => (
            "open".to_string(),
            vec!["-t".to_string(), path.display().to_string()],
        ),
    };

    #[cfg(target_os = "linux")]
    let (cmd, args) = match editor {
        Some(e) => (e, vec![path.display().to_string()]),
        None => ("xdg-open".to_string(), vec![path.display().to_string()]),
    };

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let (cmd, args) = match editor {
        Some(e) => (e, vec![path.display().to_string()]),
        None => bail!("No EDITOR environment variable set"),
    };

    let status = std::process::Command::new(&cmd)
        .args(&args)
        .status()
        .with_context(|| format!("Failed to run: {} {}", cmd, args.join(" ")))?;

    if !status.success() {
        bail!("Editor exited with error");
    }

    Ok(())
}

/// Print configuration file path
async fn path(config_override: Option<PathBuf>) -> Result<()> {
    let config_path = config_override.or_else(find_config_file);

    if let Some(path) = config_path {
        println!("{}", path.display());
    } else {
        // Print where it would be created
        let default_path = config_search_paths()
            .first()
            .map(|cp| cp.path.clone())
            .context("No config search paths available")?;
        eprintln!("# Config file does not exist. Would be created at:");
        println!("{}", default_path.display());
    }

    Ok(())
}

/// Output as CLI command arguments
async fn to_command(config_override: Option<PathBuf>) -> Result<()> {
    let config_path = config_override
        .or_else(find_config_file)
        .context("No configuration file found")?;

    let config_file = load_config(&config_path)?;
    let config = &config_file.config;

    // Get executable path
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "authsock-filter".to_string());

    print_config_as_cli(&exe, config);

    Ok(())
}

/// Print config in CLI argument format with proper shell quoting
pub fn print_config_as_cli(exe: &str, config: &crate::config::Config) {
    use std::collections::BTreeMap;

    // Group sockets by upstream (BTreeMap for stable ordering)
    let mut groups: BTreeMap<&str, Vec<(&str, &crate::config::SocketConfig)>> = BTreeMap::new();
    for (name, socket) in &config.sockets {
        let upstream = socket.upstream.as_deref().unwrap_or(&config.upstream);
        groups.entry(upstream).or_default().push((name, socket));
    }

    // Sort sockets within each group by path
    for sockets in groups.values_mut() {
        sockets.sort_by(|a, b| a.1.path.cmp(&b.1.path));
    }

    println!("{} run \\", shlex::try_quote(exe).unwrap_or(exe.into()));

    let group_count = groups.len();
    for (i, (upstream, sockets)) in groups.iter().enumerate() {
        let is_last_group = i == group_count - 1;
        println!(
            "  --upstream {} \\",
            shlex::try_quote(upstream).unwrap_or((*upstream).into())
        );

        for (j, (_name, socket)) in sockets.iter().enumerate() {
            let is_last_socket = is_last_group && j == sockets.len() - 1;

            // Quote socket path
            let quoted_path = shlex::try_quote(&socket.path).unwrap_or(socket.path.clone().into());

            // Quote each filter (flatten all groups - AND within group, OR between groups)
            // For CLI output, we concatenate all filters from all groups
            let quoted_filters: Vec<String> = socket
                .filters
                .iter()
                .flatten()
                .map(|f| shlex::try_quote(f).unwrap_or(f.clone().into()).to_string())
                .collect();

            let filters_str = quoted_filters.join(" ");
            let line_end = if is_last_socket { "" } else { " \\" };

            if filters_str.is_empty() {
                println!("    --socket {}{}", quoted_path, line_end);
            } else {
                println!("    --socket {} {}{}", quoted_path, filters_str, line_end);
            }
        }
    }
}
