//! Integration tests with real SSH keys

use authsock_filter::filter::FilterEvaluator;
use authsock_filter::protocol::Identity;
use bytes::Bytes;
use ssh_key::PublicKey;
use std::fs;
use tempfile::TempDir;

// Pre-generated test keys
const ED25519_KEY_1: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl user@work.example.com";
const ED25519_KEY_2: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHUu2eEV0kRvK3dMRlSFwHxVoNxCfwjKmAZBlhkNjC4i user@personal.example.com";
const ED25519_KEY_3: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKwfZn/9xXqbDtEzpAEZEoEBllBkLR+NpVHhMxCmyC9L dev@work.example.com";

/// Parse a public key from OpenSSH format and create an Identity
fn make_identity_from_str(key_str: &str) -> Identity {
    let public_key: PublicKey = key_str.parse().unwrap();
    let key_blob = Bytes::from(public_key.to_bytes().unwrap());
    let comment = key_str.split_whitespace().nth(2).unwrap_or("").to_string();
    Identity::new(key_blob, comment)
}

#[test]
fn test_filter_by_key_type_ed25519() {
    let ed25519_key = make_identity_from_str(ED25519_KEY_1);

    // Note: filter format uses colon (type:value), not equals
    let evaluator = FilterEvaluator::parse(&["type:ed25519".to_string()]).unwrap();

    assert!(evaluator.matches(&ed25519_key), "ed25519 key should match");
}

#[test]
fn test_filter_exclude_key_type() {
    let ed25519_key = make_identity_from_str(ED25519_KEY_1);

    let evaluator = FilterEvaluator::parse(&["-type:ed25519".to_string()]).unwrap();

    assert!(!evaluator.matches(&ed25519_key), "ed25519 key should be excluded");
}

#[test]
fn test_filter_by_comment_glob() {
    let work_key = make_identity_from_str(ED25519_KEY_1);
    let personal_key = make_identity_from_str(ED25519_KEY_2);

    let evaluator = FilterEvaluator::parse(&["comment:*@work*".to_string()]).unwrap();

    assert!(evaluator.matches(&work_key), "work key should match");
    assert!(!evaluator.matches(&personal_key), "personal key should not match");
}

#[test]
fn test_filter_by_comment_regex() {
    let work_key = make_identity_from_str(ED25519_KEY_1);
    let personal_key = make_identity_from_str(ED25519_KEY_2);

    let evaluator = FilterEvaluator::parse(&["comment:~@work\\.".to_string()]).unwrap();

    assert!(evaluator.matches(&work_key), "work key should match regex");
    assert!(!evaluator.matches(&personal_key), "personal key should not match regex");
}

#[test]
fn test_filter_exclude_comment() {
    let work_key = make_identity_from_str(ED25519_KEY_1);
    let personal_key = make_identity_from_str(ED25519_KEY_2);

    let evaluator = FilterEvaluator::parse(&["-comment:*@work*".to_string()]).unwrap();

    assert!(!evaluator.matches(&work_key), "work key should be excluded");
    assert!(evaluator.matches(&personal_key), "personal key should match");
}

#[test]
fn test_filter_multiple_rules_and() {
    let ed25519_work = make_identity_from_str(ED25519_KEY_1);
    let ed25519_personal = make_identity_from_str(ED25519_KEY_2);

    let evaluator = FilterEvaluator::parse(&[
        "type:ed25519".to_string(),
        "comment:*@work*".to_string(),
    ])
    .unwrap();

    assert!(
        evaluator.matches(&ed25519_work),
        "ed25519 work key should match both rules"
    );
    assert!(
        !evaluator.matches(&ed25519_personal),
        "ed25519 personal key should not match (wrong comment)"
    );
}

#[test]
fn test_filter_fingerprint() {
    let identity = make_identity_from_str(ED25519_KEY_1);

    // Get the fingerprint of this key
    let fingerprint = identity.fingerprint().unwrap();
    let fingerprint_str = format!("fingerprint:{}", fingerprint);

    let evaluator = FilterEvaluator::parse(&[fingerprint_str]).unwrap();
    assert!(evaluator.matches(&identity), "key should match its own fingerprint");

    // Different key should not match
    let other_identity = make_identity_from_str(ED25519_KEY_2);
    assert!(
        !evaluator.matches(&other_identity),
        "different key should not match fingerprint"
    );
}

#[test]
fn test_filter_fingerprint_auto_detect() {
    let identity = make_identity_from_str(ED25519_KEY_1);

    // Auto-detect fingerprint format (SHA256:...)
    let fingerprint = identity.fingerprint().unwrap();
    let fingerprint_str = fingerprint.to_string();

    let evaluator = FilterEvaluator::parse(&[fingerprint_str]).unwrap();
    assert!(evaluator.matches(&identity), "key should match auto-detected fingerprint");
}

#[test]
fn test_filter_keyfile() {
    let temp_dir = TempDir::new().unwrap();
    let keyfile_path = temp_dir.path().join("authorized_keys");

    // Save the key to file
    fs::write(&keyfile_path, format!("{}\n", ED25519_KEY_1)).unwrap();

    // Create identity from the same key
    let matching_identity = make_identity_from_str(ED25519_KEY_1);
    let other_identity = make_identity_from_str(ED25519_KEY_2);

    let filter_str = format!("keyfile:{}", keyfile_path.display());
    let evaluator = FilterEvaluator::parse(&[filter_str]).unwrap();

    assert!(
        evaluator.matches(&matching_identity),
        "key in keyfile should match"
    );
    assert!(
        !evaluator.matches(&other_identity),
        "key not in keyfile should not match"
    );
}

#[test]
fn test_filter_pubkey_auto_detect() {
    let identity = make_identity_from_str(ED25519_KEY_1);

    // Auto-detect pubkey format (ssh-ed25519 ...)
    let pubkey_str = ED25519_KEY_1.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
    let evaluator = FilterEvaluator::parse(&[pubkey_str]).unwrap();

    assert!(evaluator.matches(&identity), "key should match auto-detected pubkey");

    let other_identity = make_identity_from_str(ED25519_KEY_2);
    assert!(
        !evaluator.matches(&other_identity),
        "different key should not match pubkey"
    );
}

#[test]
fn test_filter_pubkey_explicit() {
    let identity = make_identity_from_str(ED25519_KEY_1);

    // Explicit pubkey: prefix
    let pubkey_str = ED25519_KEY_1.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
    let filter_str = format!("pubkey:{}", pubkey_str);
    let evaluator = FilterEvaluator::parse(&[filter_str]).unwrap();

    assert!(evaluator.matches(&identity), "key should match explicit pubkey");
}

#[test]
fn test_filter_identities_list() {
    let keys = vec![
        make_identity_from_str(ED25519_KEY_1),  // work ed25519
        make_identity_from_str(ED25519_KEY_2),  // personal ed25519
        make_identity_from_str(ED25519_KEY_3),  // work ed25519
    ];

    // Filter: ed25519 keys with work comment
    let evaluator = FilterEvaluator::parse(&[
        "type:ed25519".to_string(),
        "comment:*@work*".to_string(),
    ])
    .unwrap();

    let filtered = evaluator.filter_identities(keys);
    assert_eq!(filtered.len(), 2, "should have 2 work ed25519 keys");
    assert!(filtered.iter().all(|k| k.comment.contains("@work")));
}

#[test]
fn test_filter_empty_allows_all() {
    let keys = vec![
        make_identity_from_str(ED25519_KEY_1),
        make_identity_from_str(ED25519_KEY_2),
    ];

    let evaluator = FilterEvaluator::default();
    let filtered = evaluator.filter_identities(keys.clone());

    assert_eq!(filtered.len(), 2, "empty filter should allow all keys");
}

#[test]
fn test_complex_filter_scenario() {
    // Scenario: Allow ed25519 keys for work only
    let work_ed25519 = make_identity_from_str(ED25519_KEY_1);
    let personal_ed25519 = make_identity_from_str(ED25519_KEY_2);
    let work_ed25519_2 = make_identity_from_str(ED25519_KEY_3);

    // Filter: ed25519 AND work comment
    let evaluator = FilterEvaluator::parse(&[
        "type:ed25519".to_string(),
        "comment:*@work*".to_string(),
    ])
    .unwrap();

    assert!(evaluator.matches(&work_ed25519));
    assert!(!evaluator.matches(&personal_ed25519));
    assert!(evaluator.matches(&work_ed25519_2));
}

#[test]
fn test_filter_multiple_negations() {
    // Scenario: Exclude both work and personal (should match nothing in our set)
    let work_key = make_identity_from_str(ED25519_KEY_1);
    let personal_key = make_identity_from_str(ED25519_KEY_2);

    let evaluator = FilterEvaluator::parse(&[
        "-comment:*@work*".to_string(),
        "-comment:*@personal*".to_string(),
    ])
    .unwrap();

    assert!(!evaluator.matches(&work_key), "work key should be excluded");
    assert!(!evaluator.matches(&personal_key), "personal key should be excluded");
}

#[test]
fn test_filter_comment_exact_match() {
    let key = make_identity_from_str(ED25519_KEY_1);

    // Exact match
    let evaluator = FilterEvaluator::parse(&["comment:user@work.example.com".to_string()]).unwrap();
    assert!(evaluator.matches(&key), "should match exact comment");

    // Non-matching exact
    let evaluator2 = FilterEvaluator::parse(&["comment:other@work.example.com".to_string()]).unwrap();
    assert!(!evaluator2.matches(&key), "should not match different comment");
}

#[test]
fn test_filter_key_type_variations() {
    let key = make_identity_from_str(ED25519_KEY_1);

    // Various type specifications
    let evaluator1 = FilterEvaluator::parse(&["type:ed25519".to_string()]).unwrap();
    assert!(evaluator1.matches(&key));

    let evaluator2 = FilterEvaluator::parse(&["type:rsa".to_string()]).unwrap();
    assert!(!evaluator2.matches(&key));

    let evaluator3 = FilterEvaluator::parse(&["type:ecdsa".to_string()]).unwrap();
    assert!(!evaluator3.matches(&key));
}
