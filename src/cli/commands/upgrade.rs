//! Upgrade command - upgrade to the latest version from GitHub

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tracing::info;

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
    let arch_name = match arch {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        "arm" => "arm",
        _ => arch,
    };

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

    // Fetch release information from GitHub
    let release = if let Some(target_version) = &args.version {
        // Get specific version
        fetch_release(Some(target_version)).await?
    } else {
        // Get latest version
        fetch_release(None).await?
    };

    let latest_version = release.tag_name.trim_start_matches('v');
    println!("Latest version:  {}", latest_version);
    println!("Release:         {}", release.name);
    println!("Published:       {}", release.published_at);
    println!("URL:             {}", release.html_url);
    println!();

    // Check if upgrade is needed
    let comparison = compare_versions(current_version, latest_version);
    let needs_upgrade = match comparison {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Equal => {
            if args.force {
                println!("Already at version {}, but --force specified.", latest_version);
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
            println!("An update is available: {} -> {}", current_version, latest_version);
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

    // TODO: Implement actual download and installation
    // 1. Download the asset to a temporary file
    // 2. Verify checksum if available
    // 3. Replace the current executable
    // 4. Handle platform-specific installation (chmod +x on Unix)

    /*
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

    let bytes = response.bytes().await.context("Failed to read download")?;

    // Get current executable path
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;

    // Write to temporary file first
    let temp_path = current_exe.with_extension("new");
    std::fs::write(&temp_path, &bytes).context("Failed to write new executable")?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&temp_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&temp_path, perms)?;
    }

    // Replace the executable
    std::fs::rename(&temp_path, &current_exe).context("Failed to replace executable")?;

    println!("Successfully upgraded to version {}", latest_version);
    */

    println!();
    println!("[STUB] Download and installation not yet implemented.");
    println!("Please download manually from: {}", asset.browser_download_url);

    Ok(())
}

/// Fetch release information from GitHub
async fn fetch_release(version: Option<&String>) -> Result<GitHubRelease> {
    let url = if let Some(v) = version {
        let tag = if v.starts_with('v') {
            v.clone()
        } else {
            format!("v{}", v)
        };
        format!(
            "https://api.github.com/repos/{}/{}/releases/tags/{}",
            GITHUB_OWNER, GITHUB_REPO, tag
        )
    } else {
        format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            GITHUB_OWNER, GITHUB_REPO
        )
    };

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
            if let Some(v) = version {
                bail!("Release version {} not found", v);
            } else {
                bail!("No releases found for this repository");
            }
        }
        bail!("GitHub API error: HTTP {}", response.status());
    }

    let release: GitHubRelease = response
        .json()
        .await
        .context("Failed to parse release information")?;

    Ok(release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_versions() {
        assert_eq!(
            compare_versions("0.1.0", "0.2.0"),
            std::cmp::Ordering::Less
        );
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
