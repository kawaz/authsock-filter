//! Version manager detection and executable resolution utilities

use std::path::{Path, PathBuf};

/// Information about a detected version manager
#[derive(Debug, Clone)]
pub struct VersionManagerInfo {
    pub name: &'static str,
    pub version_path: Option<String>,
}

/// Version manager path patterns (install paths)
const INSTALL_PATTERNS: &[(&str, &str)] = &[
    ("/mise/installs/", "mise"),
    ("/.mise/installs/", "mise"),
    ("/asdf/installs/", "asdf"),
    ("/.asdf/installs/", "asdf"),
    ("/nix/store/", "nix"),
    ("/.nix-profile/", "nix"),
    ("/Cellar/", "homebrew"),
];

/// Shim path patterns
const SHIM_PATTERNS: &[&str] = &[
    "/mise/shims/",
    "/.mise/shims/",
    "/asdf/shims/",
    "/.asdf/shims/",
];

/// Known shim directories relative to home
const SHIM_DIRS: &[&str] = &[
    ".local/share/mise/shims",
    ".mise/shims",
    ".asdf/shims",
    ".nix-profile/bin",
];

/// Detect if a path is under a version manager
pub fn detect_version_manager(path: &Path) -> Option<VersionManagerInfo> {
    let path_str = path.to_string_lossy();

    // Check for temporary/build paths first
    if path_str.contains("/target/debug/") || path_str.contains("/target/release/") {
        return Some(VersionManagerInfo {
            name: "temporary",
            version_path: None,
        });
    }

    for (pattern, name) in INSTALL_PATTERNS {
        if let Some(idx) = path_str.find(pattern) {
            let after = &path_str[idx + pattern.len()..];
            let version = after.split('/').next().map(String::from);
            return Some(VersionManagerInfo {
                name,
                version_path: version,
            });
        }
    }

    None
}

/// Check if path is a known shim location
pub fn is_shim_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    SHIM_PATTERNS.iter().any(|p| path_str.contains(p))
}

/// Check if a path is an executable file
#[cfg(unix)]
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Check if path exists and is executable, return the path if valid
pub fn check_executable(path: &Path) -> Option<PathBuf> {
    if is_executable(path) {
        Some(path.to_path_buf())
    } else {
        None
    }
}

/// Find executable candidates in PATH and known shim locations
pub fn find_executable_candidates(name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Check PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let candidate = PathBuf::from(dir).join(name);
            if let Some(path) = check_executable(&candidate)
                && seen.insert(path.clone())
            {
                candidates.push(path);
            }
        }
    }

    // Check known shim locations
    if let Some(home) = dirs::home_dir() {
        for shim_dir in SHIM_DIRS {
            let candidate = home.join(shim_dir).join(name);
            if let Some(path) = check_executable(&candidate)
                && seen.insert(path.clone())
            {
                candidates.push(path);
            }
        }
    }

    candidates
}

/// Resolve what binary a shim points to
pub fn resolve_shim_executable(shim_path: &Path) -> Option<PathBuf> {
    let name = shim_path.file_name()?.to_str()?;
    let shim_str = shim_path.to_string_lossy();

    // Try version manager's which command
    let which_result = if shim_str.contains("/mise/shims/") || shim_str.contains("/.mise/shims/") {
        std::process::Command::new("mise")
            .args(["which", name])
            .output()
            .ok()
    } else if shim_str.contains("/asdf/shims/") || shim_str.contains("/.asdf/shims/") {
        std::process::Command::new("asdf")
            .args(["which", name])
            .output()
            .ok()
    } else {
        None
    };

    if let Some(output) = which_result
        && output.status.success()
    {
        let path_str = String::from_utf8_lossy(&output.stdout);
        let path = PathBuf::from(path_str.trim());
        if path.exists() {
            return Some(path);
        }
    }

    // Fallback: execute with version command
    let output = std::process::Command::new(shim_path)
        .arg("version")
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(path_str) = line.strip_prefix("  Executable: ") {
                return Some(PathBuf::from(path_str.trim()));
            }
        }
    }

    None
}

/// Find shim path suggestions for an executable
pub fn find_shim_suggestions(name: &str) -> Vec<PathBuf> {
    find_executable_candidates(name)
        .into_iter()
        .filter(|p| is_shim_path(p))
        .collect()
}
