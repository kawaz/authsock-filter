//! Command implementations for authsock-filter CLI

pub mod completion;
pub mod config;
pub mod run;
pub mod service;
pub mod version;

use std::path::{Path, PathBuf};

// ============================================================================
// Version manager detection (shared between service and upgrade commands)
// ============================================================================

/// Information about a detected version manager
pub struct VersionManagerInfo {
    pub name: &'static str,
    pub current_path: PathBuf,
    pub suggestions: Vec<(PathBuf, bool)>,
}

/// Check if the path is under a version manager
pub fn detect_version_manager(path: &Path) -> Option<VersionManagerInfo> {
    let path_str = path.to_string_lossy();

    // Known version manager patterns
    let version_manager_patterns = [
        ("/mise/installs/", "mise"),
        ("/.mise/installs/", "mise"),
        ("/asdf/installs/", "asdf"),
        ("/.asdf/installs/", "asdf"),
        ("/aqua/pkgs/", "aqua"),
        ("/.aqua/pkgs/", "aqua"),
        ("/nix/store/", "nix"),
    ];

    for (pattern, manager) in version_manager_patterns {
        if path_str.contains(pattern) {
            let suggestions = find_shim_suggestions(path);
            return Some(VersionManagerInfo {
                name: manager,
                current_path: path.to_path_buf(),
                suggestions,
            });
        }
    }

    None
}

/// Find available shim paths for authsock-filter and check if they point to the same binary
fn find_shim_suggestions(current_exe: &Path) -> Vec<(PathBuf, bool)> {
    let mut suggestions = Vec::new();

    // Check common shim/bin locations
    let candidates = [
        // mise shims (XDG_DATA_HOME or ~/.local/share)
        dirs::home_dir().map(|d| d.join(".local/share/mise/shims/authsock-filter")),
        // mise shims (alternative location on some systems)
        dirs::data_local_dir().map(|d| d.join("mise/shims/authsock-filter")),
        // asdf shims
        dirs::home_dir().map(|d| d.join(".asdf/shims/authsock-filter")),
        // nix profile
        dirs::home_dir().map(|d| d.join(".nix-profile/bin/authsock-filter")),
        // NixOS system profile
        Some(PathBuf::from("/run/current-system/sw/bin/authsock-filter")),
        // ~/.local/bin (common user bin)
        dirs::home_dir().map(|d| d.join(".local/bin/authsock-filter")),
        // Homebrew
        Some(PathBuf::from("/opt/homebrew/bin/authsock-filter")),
        Some(PathBuf::from("/usr/local/bin/authsock-filter")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() && is_executable(&candidate) {
            let is_same = is_same_binary(&candidate, current_exe);
            suggestions.push((candidate, is_same));
        }
    }

    suggestions
}

/// Check if a shim/symlink points to the same binary as current executable
fn is_same_binary(shim_path: &Path, current_exe: &Path) -> bool {
    // First, try to resolve symlinks
    let resolved_shim = shim_path.canonicalize().ok();
    let resolved_current = current_exe.canonicalize().ok();

    if let (Some(shim), Some(current)) = (&resolved_shim, &resolved_current)
        && shim == current
    {
        return true;
    }

    // For mise shims, try running `mise which authsock-filter` to get the actual path
    if shim_path.to_string_lossy().contains("/mise/shims/")
        && let Ok(output) = std::process::Command::new("mise")
            .args(["which", "authsock-filter"])
            .output()
        && output.status.success()
    {
        let mise_path = String::from_utf8_lossy(&output.stdout);
        let mise_path = PathBuf::from(mise_path.trim());
        if let Ok(resolved_mise) = mise_path.canonicalize()
            && let Some(resolved_current) = &resolved_current
        {
            return &resolved_mise == resolved_current;
        }
    }

    false
}

/// Check if a file is executable
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.exists()
}
