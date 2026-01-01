//! Keyfile matching filter (authorized_keys format)

use crate::error::{Error, Result};
use crate::filter::PubkeyMatcher;
use crate::protocol::Identity;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Matcher for keys from an authorized_keys style file
#[derive(Debug, Clone)]
pub struct KeyfileMatcher {
    /// Path to the keyfile
    path: PathBuf,
    /// Cached key matchers
    matchers: Arc<RwLock<Vec<PubkeyMatcher>>>,
}

impl KeyfileMatcher {
    /// Create a new keyfile matcher
    pub fn new(path: &str) -> Result<Self> {
        let expanded = shellexpand::tilde(path);
        let path = PathBuf::from(expanded.as_ref());

        let matcher = Self {
            path,
            matchers: Arc::new(RwLock::new(Vec::new())),
        };

        // Load keys initially
        matcher.reload()?;

        Ok(matcher)
    }

    /// Get the path being matched
    pub fn path(&self) -> String {
        self.path.display().to_string()
    }

    /// Reload keys from the file
    pub fn reload(&self) -> Result<()> {
        let keys = Self::load_keys(&self.path)?;
        let mut matchers = self
            .matchers
            .write()
            .map_err(|e| Error::Filter(format!("Failed to acquire lock: {}", e)))?;
        *matchers = keys;
        Ok(())
    }

    /// Load keys from a file
    fn load_keys(path: &Path) -> Result<Vec<PubkeyMatcher>> {
        let content = fs::read_to_string(path).map_err(|e| {
            Error::Filter(format!(
                "Failed to read keyfile '{}': {}",
                path.display(),
                e
            ))
        })?;

        let mut matchers = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Try to parse as a public key
            // authorized_keys format may have options prefix, so we try to find the key part
            if let Some(key_part) = Self::extract_key_part(line) {
                match PubkeyMatcher::new(key_part) {
                    Ok(m) => matchers.push(m),
                    Err(e) => {
                        tracing::warn!("Skipping invalid key in {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(matchers)
    }

    /// Extract the key part from an authorized_keys line
    /// Handles optional options prefix
    fn extract_key_part(line: &str) -> Option<&str> {
        // Key types we recognize
        let key_prefixes = [
            "ssh-ed25519",
            "ssh-rsa",
            "ssh-dss",
            "ecdsa-sha2-",
            "sk-ssh-ed25519",
            "sk-ecdsa-sha2-",
        ];

        // Find where the key starts
        for prefix in &key_prefixes {
            if let Some(pos) = line.find(prefix) {
                return Some(&line[pos..]);
            }
        }

        // If no recognized prefix, assume the whole line is a key
        Some(line)
    }

    /// Check if this matcher matches the given identity
    pub fn matches(&self, identity: &Identity) -> bool {
        if let Ok(matchers) = self.matchers.read() {
            matchers.iter().any(|m| m.matches(identity))
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_extract_key_part() {
        let line = "ssh-ed25519 AAAAC3 comment";
        assert_eq!(KeyfileMatcher::extract_key_part(line), Some(line));

        let line_with_options = "no-agent-forwarding ssh-ed25519 AAAAC3 comment";
        assert_eq!(
            KeyfileMatcher::extract_key_part(line_with_options),
            Some("ssh-ed25519 AAAAC3 comment")
        );
    }

    #[test]
    fn test_load_keys() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "# Comment line").unwrap();
        writeln!(file, "").unwrap();
        writeln!(file, "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl test@example.com").unwrap();

        let matcher = KeyfileMatcher::new(file.path().to_str().unwrap()).unwrap();
        let matchers = matcher.matchers.read().unwrap();
        assert_eq!(matchers.len(), 1);
    }
}
