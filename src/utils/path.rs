//! Path expansion utilities

use std::path::PathBuf;

/// Expand environment variables and tilde in a path string
pub fn expand_path(path: &str) -> crate::Result<String> {
    shellexpand::full(path)
        .map(|s| s.into_owned())
        .map_err(|e| crate::Error::Config(format!("Failed to expand path '{}': {}", path, e)))
}

/// Expand path and convert to PathBuf
pub fn expand_to_pathbuf(path: &str) -> crate::Result<PathBuf> {
    expand_path(path).map(PathBuf::from)
}
