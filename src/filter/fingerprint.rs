//! Fingerprint matching filter

use crate::error::{Error, Result};
use crate::protocol::Identity;

/// Matcher for SSH key fingerprints
#[derive(Debug, Clone)]
pub struct FingerprintMatcher {
    /// The fingerprint pattern to match
    pattern: String,
}

impl FingerprintMatcher {
    /// Create a new fingerprint matcher
    pub fn new(pattern: &str) -> Result<Self> {
        // Validate format
        if !pattern.starts_with("SHA256:") && !pattern.starts_with("MD5:") {
            return Err(Error::Filter(format!(
                "Invalid fingerprint format: {}. Expected SHA256:... or MD5:...",
                pattern
            )));
        }
        Ok(Self {
            pattern: pattern.to_string(),
        })
    }

    /// Get the pattern being matched
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Check if this matcher matches the given identity
    pub fn matches(&self, identity: &Identity) -> bool {
        if let Some(fp) = identity.fingerprint() {
            let fp_str = fp.to_string();
            // Support prefix matching for convenience
            fp_str.starts_with(&self.pattern) || self.pattern == fp_str
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_sha256_fingerprint() {
        let matcher = FingerprintMatcher::new("SHA256:abc123").unwrap();
        assert_eq!(matcher.pattern(), "SHA256:abc123");
    }

    #[test]
    fn test_valid_md5_fingerprint() {
        let matcher = FingerprintMatcher::new("MD5:ab:cd:ef").unwrap();
        assert_eq!(matcher.pattern(), "MD5:ab:cd:ef");
    }

    #[test]
    fn test_invalid_fingerprint() {
        let result = FingerprintMatcher::new("invalid");
        assert!(result.is_err());
    }
}
