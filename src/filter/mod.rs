//! Filter module for key filtering
//!
//! This module provides various filter types for SSH keys:
//! - Fingerprint matching
//! - Comment matching (exact, glob, regex)
//! - Key type matching
//! - Public key matching
//! - Keyfile matching (authorized_keys format)
//! - GitHub user keys matching
//! - Negation

mod comment;
mod evaluator;
mod fingerprint;
mod github;
mod keyfile;
mod keytype;
mod pubkey;
mod rule;

pub use comment::CommentMatcher;
pub use evaluator::FilterEvaluator;
pub use fingerprint::FingerprintMatcher;
pub use github::GitHubKeysMatcher;
pub use keyfile::KeyfileMatcher;
pub use keytype::KeyTypeMatcher;
pub use pubkey::PubkeyMatcher;
pub use rule::{Filter, FilterRule};
