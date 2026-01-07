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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_path_tilde() {
        let result = expand_path("~/test").unwrap();
        assert!(result.starts_with('/'));
        assert!(result.ends_with("/test"));
        assert!(!result.contains('~'));
    }

    #[test]
    fn test_expand_path_env_var() {
        // Use HOME which is always set
        let home = std::env::var("HOME").unwrap();
        let result = expand_path("$HOME/test").unwrap();
        assert_eq!(result, format!("{}/test", home));
    }

    #[test]
    fn test_expand_path_traversal_preserved() {
        // Path traversal sequences are preserved (not sanitized)
        // This is intentional - the user controls config files
        let result = expand_path("../../../etc/passwd").unwrap();
        assert_eq!(result, "../../../etc/passwd");

        let result = expand_path("/tmp/../etc/passwd").unwrap();
        assert_eq!(result, "/tmp/../etc/passwd");
    }

    #[test]
    fn test_expand_path_absolute() {
        let result = expand_path("/absolute/path").unwrap();
        assert_eq!(result, "/absolute/path");
    }

    #[test]
    fn test_expand_to_pathbuf() {
        let result = expand_to_pathbuf("/tmp/test.sock").unwrap();
        assert_eq!(result, PathBuf::from("/tmp/test.sock"));
    }
}
