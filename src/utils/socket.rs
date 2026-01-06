//! Unix socket utility functions
//!
//! Provides common operations for Unix socket management including
//! safe removal, directory creation, and permission setting.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Error type for socket operations
#[derive(Debug, thiserror::Error)]
pub enum SocketError {
    #[error("Refusing to replace symlink at {path}: potential security risk")]
    SymlinkDetected { path: String },

    #[error("Failed to check existing socket at {path}: {source}")]
    MetadataError { path: String, source: io::Error },

    #[error("Failed to remove existing socket at {path}: {source}")]
    RemoveError { path: String, source: io::Error },

    #[error("Failed to create directory {path}: {source}")]
    CreateDirError { path: String, source: io::Error },

    #[error("Failed to set permissions on socket at {path}: {source}")]
    PermissionError { path: String, source: io::Error },
}

/// Safely remove an existing socket file if present.
///
/// This function uses `symlink_metadata` instead of `exists` to prevent
/// TOCTOU race conditions. If the path is a symlink, it returns an error
/// to prevent potential symlink attacks.
///
/// # Errors
/// - Returns `SymlinkDetected` if the path is a symbolic link
/// - Returns `MetadataError` if metadata cannot be read (except NotFound)
/// - Returns `RemoveError` if the file cannot be removed
pub fn remove_existing_socket(path: &Path) -> Result<(), SocketError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            // Check if it's a symlink (potential attack)
            if metadata.file_type().is_symlink() {
                return Err(SocketError::SymlinkDetected {
                    path: path.display().to_string(),
                });
            }
            fs::remove_file(path).map_err(|e| SocketError::RemoveError {
                path: path.display().to_string(),
                source: e,
            })?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // File doesn't exist, which is fine
        }
        Err(e) => {
            return Err(SocketError::MetadataError {
                path: path.display().to_string(),
                source: e,
            });
        }
    }
    Ok(())
}

/// Ensure the parent directory of a path exists.
///
/// Creates the parent directory and all ancestors if they don't exist.
///
/// # Errors
/// Returns `CreateDirError` if directory creation fails.
pub fn ensure_parent_dir(path: &Path) -> Result<(), SocketError> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| SocketError::CreateDirError {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    Ok(())
}

/// Set socket permissions to owner read/write only (0600).
///
/// This should be called immediately after binding a Unix socket
/// to ensure only the owner can connect.
///
/// # Errors
/// Returns `PermissionError` if permissions cannot be set.
pub fn set_socket_permissions(path: &Path) -> Result<(), SocketError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| {
        SocketError::PermissionError {
            path: path.display().to_string(),
            source: e,
        }
    })
}

/// Prepare a path for socket binding.
///
/// This is a convenience function that:
/// 1. Removes any existing socket file (with symlink protection)
/// 2. Creates the parent directory if needed
///
/// Call `set_socket_permissions` after binding the socket.
///
/// # Errors
/// Returns an error if any of the preparation steps fail.
pub fn prepare_socket_path(path: &Path) -> Result<(), SocketError> {
    remove_existing_socket(path)?;
    ensure_parent_dir(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    #[test]
    fn test_remove_existing_socket_not_found() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.sock");
        assert!(remove_existing_socket(&path).is_ok());
    }

    #[test]
    fn test_remove_existing_socket_regular_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");
        fs::write(&path, b"test").unwrap();
        assert!(path.exists());
        assert!(remove_existing_socket(&path).is_ok());
        assert!(!path.exists());
    }

    #[test]
    fn test_remove_existing_socket_symlink_rejected() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join("link.sock");
        fs::write(&target, b"target").unwrap();
        symlink(&target, &link).unwrap();

        let result = remove_existing_socket(&link);
        assert!(matches!(result, Err(SocketError::SymlinkDetected { .. })));
        // Link should still exist (not removed)
        assert!(link.symlink_metadata().is_ok());
    }

    #[test]
    fn test_ensure_parent_dir_exists() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("existing").join("test.sock");
        fs::create_dir(dir.path().join("existing")).unwrap();
        assert!(ensure_parent_dir(&path).is_ok());
    }

    #[test]
    fn test_ensure_parent_dir_creates() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("new").join("nested").join("test.sock");
        assert!(ensure_parent_dir(&path).is_ok());
        assert!(dir.path().join("new").join("nested").exists());
    }

    #[test]
    fn test_set_socket_permissions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");
        fs::write(&path, b"test").unwrap();
        assert!(set_socket_permissions(&path).is_ok());
        let perms = fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn test_prepare_socket_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("subdir").join("test.sock");
        // Create an existing file
        fs::create_dir(dir.path().join("subdir")).unwrap();
        fs::write(&path, b"old").unwrap();

        assert!(prepare_socket_path(&path).is_ok());
        assert!(!path.exists()); // Old file removed
        assert!(dir.path().join("subdir").exists()); // Dir still exists
    }
}
