//! Upgrade command - upgrade to the latest version from GitHub

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tracing::info;

use super::detect_version_manager;
use crate::cli::args::UpgradeArgs;

/// GitHub API release information
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: String,
    html_url: String,
    published_at: String,
    assets: Vec<GitHubAsset>,
    body: String,
}

/// GitHub release asset
#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Repository information
const GITHUB_OWNER: &str = "kawaz";
const GITHUB_REPO: &str = "authsock-filter";

/// Get the appropriate asset name for the current platform
fn get_platform_asset_name() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    // Map architecture names
    let arch_name = arch;

    // Map OS names
    let os_name = match os {
        "macos" => "apple-darwin",
        "linux" => "unknown-linux-gnu",
        "windows" => "pc-windows-msvc",
        _ => os,
    };

    format!("authsock-filter-{}-{}", arch_name, os_name)
}

/// Compare version strings (simple semver comparison)
fn compare_versions(current: &str, latest: &str) -> std::cmp::Ordering {
    let current = current.trim_start_matches('v');
    let latest = latest.trim_start_matches('v');

    let parse_version = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| p.split('-').next())
            .filter_map(|p| p.parse().ok())
            .collect()
    };

    let current_parts = parse_version(current);
    let latest_parts = parse_version(latest);

    for (c, l) in current_parts.iter().zip(latest_parts.iter()) {
        match c.cmp(l) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    current_parts.len().cmp(&latest_parts.len())
}

/// Execute the upgrade command
pub async fn execute(args: UpgradeArgs) -> Result<()> {
    let current_version = crate::VERSION;
    info!(current = current_version, "Checking for updates...");

    println!("Current version: {}", current_version);
    println!();

    // Fetch latest release information from GitHub
    let release = fetch_release().await?;

    let latest_version = release.tag_name.trim_start_matches('v');
    println!("Latest version:  {}", latest_version);
    println!("Release:         {}", release.name);
    println!("Published:       {}", release.published_at);
    println!("URL:             {}", release.html_url);
    println!();

    // Check if running from a version-managed path
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;
    if let Some(info) = detect_version_manager(&current_exe) {
        // Extract tool name from path for mise (e.g., github-kawaz-authsock-filter -> github:kawaz/authsock-filter)
        let tool_name = extract_mise_tool_name(&info.current_path);

        let mut msg = format!(
            "Cannot upgrade - running from {} version manager.\n\
             Current path: {}\n\n\
             The 'upgrade' command directly overwrites the executable, which would\n\
             bypass {} version management and cause inconsistencies.\n",
            info.name,
            info.current_path.display(),
            info.name
        );

        msg.push_str(&format!("\nUse {} to upgrade instead:\n", info.name));

        match info.name {
            "mise" => {
                if let Some(ref name) = tool_name {
                    msg.push_str(&format!("  mise upgrade {}\n", name));
                    msg.push_str("  # or\n");
                    msg.push_str(&format!("  mise use {}@latest\n", name));
                } else {
                    msg.push_str("  mise upgrade <tool-name>\n");
                    msg.push_str("  # or\n");
                    msg.push_str("  mise use <tool-name>@latest\n");
                }
            }
            "asdf" => {
                msg.push_str("  asdf install authsock-filter latest\n");
                msg.push_str("  asdf global authsock-filter latest\n");
            }
            "aqua" => {
                msg.push_str("  aqua update authsock-filter\n");
            }
            _ => {
                msg.push_str(&format!("  {} upgrade authsock-filter\n", info.name));
            }
        }

        if !info.suggestions.is_empty() {
            msg.push_str("\nAlternatively, run upgrade from a stable path:\n");
            for (shim_path, _) in &info.suggestions {
                msg.push_str(&format!("  {} upgrade\n", shim_path.display()));
            }
        }

        bail!("{}", msg);
    }

    // Check if upgrade is needed
    let comparison = compare_versions(current_version, latest_version);
    let needs_upgrade = match comparison {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Equal => {
            if args.force {
                println!(
                    "Already at version {}, but --force specified.",
                    latest_version
                );
                true
            } else {
                println!("Already at the latest version.");
                return Ok(());
            }
        }
        std::cmp::Ordering::Greater => {
            if args.force {
                println!(
                    "Current version ({}) is newer than target ({}), but --force specified.",
                    current_version, latest_version
                );
                true
            } else {
                println!(
                    "Current version ({}) is newer than latest ({}).",
                    current_version, latest_version
                );
                return Ok(());
            }
        }
    };

    if args.check {
        // Check only mode
        if needs_upgrade {
            println!();
            println!(
                "An update is available: {} -> {}",
                current_version, latest_version
            );
            println!();
            println!("Release notes:");
            println!("{}", release.body);
        }
        return Ok(());
    }

    // Find the appropriate asset for this platform
    let asset_prefix = get_platform_asset_name();
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.starts_with(&asset_prefix))
        .with_context(|| {
            format!(
                "No pre-built binary found for this platform ({}). \
                 You may need to build from source.",
                asset_prefix
            )
        })?;

    println!("Asset:           {} ({} bytes)", asset.name, asset.size);
    println!();

    // Confirm upgrade
    if !args.yes {
        println!("Do you want to upgrade? [y/N] ");
        // TODO: Read user input
        // For now, require --yes flag
        bail!("Use --yes to confirm the upgrade");
    }

    // Download and install
    println!("Downloading {}...", asset.name);

    let client = reqwest::Client::new();
    let response = client
        .get(&asset.browser_download_url)
        .header("User-Agent", format!("authsock-filter/{}", crate::VERSION))
        .send()
        .await
        .context("Failed to download asset")?;

    if !response.status().is_success() {
        bail!("Download failed: HTTP {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(asset.size);
    println!("Download size: {} bytes", total_size);

    let bytes = response.bytes().await.context("Failed to read download")?;
    println!("Downloaded {} bytes", bytes.len());

    // Get current executable path
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;
    info!(path = %current_exe.display(), "Current executable path");

    // Create backup of current executable
    let backup_path = current_exe.with_extension("bak");
    if current_exe.exists() {
        std::fs::copy(&current_exe, &backup_path)
            .context("Failed to create backup of current executable")?;
        info!(path = %backup_path.display(), "Created backup");
    }

    // Write to temporary file first
    let temp_path = current_exe.with_extension("new");
    std::fs::write(&temp_path, &bytes).context("Failed to write new executable")?;
    info!(path = %temp_path.display(), "Wrote new executable");

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&temp_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&temp_path, perms)?;
        info!("Set executable permissions");
    }

    // Replace the executable
    std::fs::rename(&temp_path, &current_exe).context("Failed to replace executable")?;
    info!("Replaced executable");

    // Remove backup on success
    if backup_path.exists() {
        let _ = std::fs::remove_file(&backup_path);
    }

    println!();
    println!("Successfully upgraded to version {}", latest_version);
    println!();
    println!("Please restart any running instances of authsock-filter.");

    Ok(())
}

/// Fetch latest release information from GitHub
async fn fetch_release() -> Result<GitHubRelease> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    );

    info!(url = %url, "Fetching release information");

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", format!("authsock-filter/{}", crate::VERSION))
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("Failed to fetch release information")?;

    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("No releases found for this repository");
        }
        bail!("GitHub API error: HTTP {}", response.status());
    }

    let release: GitHubRelease = response
        .json()
        .await
        .context("Failed to parse release information")?;

    Ok(release)
}

/// Extract mise tool name from installation path
/// e.g., /path/mise/installs/github-kawaz-authsock-filter/0.1.5/... -> github:kawaz/authsock-filter
fn extract_mise_tool_name(path: &std::path::Path) -> Option<String> {
    let path_str = path.to_string_lossy();

    // Find the tool name segment after /mise/installs/
    let patterns = ["/mise/installs/", "/.mise/installs/"];

    for pattern in patterns {
        if let Some(start) = path_str.find(pattern) {
            let after_pattern = &path_str[start + pattern.len()..];
            // Get the first path segment (tool name with version info)
            if let Some(end) = after_pattern.find('/') {
                let tool_segment = &after_pattern[..end];
                // Convert github-user-repo format to github:user/repo
                // e.g., github-kawaz-authsock-filter -> github:kawaz/authsock-filter
                if let Some(rest) = tool_segment.strip_prefix("github-") {
                    // skip "github-"
                    if let Some(dash_pos) = rest.find('-') {
                        let user = &rest[..dash_pos];
                        let repo = &rest[dash_pos + 1..];
                        return Some(format!("github:{}/{}", user, repo));
                    }
                }
                // Return as-is for other formats
                return Some(tool_segment.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_versions() {
        assert_eq!(compare_versions("0.1.0", "0.2.0"), std::cmp::Ordering::Less);
        assert_eq!(
            compare_versions("1.0.0", "0.9.9"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_versions("1.0.0", "1.0.0"),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            compare_versions("v1.0.0", "1.0.0"),
            std::cmp::Ordering::Equal
        );
    }
}
