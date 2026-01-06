//! End-to-end proxy filtering tests with mock SSH agent

use authsock_filter::agent::{Proxy, Upstream};
use authsock_filter::filter::FilterEvaluator;
use authsock_filter::protocol::{AgentCodec, AgentMessage, Identity, MessageType};
use bytes::Bytes;
use ssh_key::PublicKey;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::{UnixListener, UnixStream};

// Pre-generated test keys (same as integration_test.rs)
const ED25519_KEY_WORK: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl user@work.example.com";
const ED25519_KEY_PERSONAL: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHUu2eEV0kRvK3dMRlSFwHxVoNxCfwjKmAZBlhkNjC4i user@personal.example.com";
const ED25519_KEY_DEV: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKwfZn/9xXqbDtEzpAEZEoEBllBkLR+NpVHhMxCmyC9L dev@work.example.com";

fn make_identity(key_str: &str) -> Identity {
    let public_key: PublicKey = key_str.parse().unwrap();
    let key_blob = Bytes::from(public_key.to_bytes().unwrap());
    let comment = key_str.split_whitespace().nth(2).unwrap_or("").to_string();
    Identity::new(key_blob, comment)
}

/// Start a mock SSH agent that returns the specified identities
async fn start_mock_agent(socket_path: &std::path::Path, identities: Vec<Identity>) {
    let listener = UnixListener::bind(socket_path).unwrap();

    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            let identities = identities.clone();
            tokio::spawn(async move {
                let (mut reader, mut writer) = stream.split();
                loop {
                    let msg = match AgentCodec::read(&mut reader).await {
                        Ok(Some(msg)) => msg,
                        _ => break,
                    };

                    let response = match msg.msg_type {
                        MessageType::RequestIdentities => {
                            AgentMessage::build_identities_answer(&identities)
                        }
                        _ => AgentMessage::failure(),
                    };

                    if AgentCodec::write(&mut writer, &response).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    // Wait for the socket to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
}

/// Start a proxy server
async fn start_proxy_server(socket_path: &std::path::Path, proxy: Arc<Proxy>) {
    let listener = UnixListener::bind(socket_path).unwrap();

    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            let proxy = proxy.clone();
            tokio::spawn(async move {
                let _ = proxy.handle_client(stream).await;
            });
        }
    });

    // Wait for the socket to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
}

/// Connect to an agent and request identities
async fn request_identities(socket_path: &std::path::Path) -> Vec<Identity> {
    let mut stream = UnixStream::connect(socket_path).await.unwrap();
    let (mut reader, mut writer) = stream.split();

    // Send REQUEST_IDENTITIES
    let request = AgentMessage::new(MessageType::RequestIdentities, Bytes::new());
    AgentCodec::write(&mut writer, &request).await.unwrap();

    // Read response
    let response = AgentCodec::read(&mut reader).await.unwrap().unwrap();
    response.parse_identities().unwrap()
}

#[tokio::test]
async fn test_proxy_filters_by_comment() {
    let temp_dir = TempDir::new().unwrap();
    let upstream_path = temp_dir.path().join("upstream.sock");
    let proxy_path = temp_dir.path().join("proxy.sock");

    // Create test identities
    let identities = vec![
        make_identity(ED25519_KEY_WORK),
        make_identity(ED25519_KEY_PERSONAL),
        make_identity(ED25519_KEY_DEV),
    ];

    // Start mock upstream agent
    start_mock_agent(&upstream_path, identities).await;

    // Create filter: only allow work keys
    let filter = FilterEvaluator::parse(&["comment=*@work*".to_string()]).unwrap();
    let upstream = Upstream::new(upstream_path.to_str().unwrap());
    let proxy = Arc::new(Proxy::new(upstream, filter));

    // Start proxy server
    start_proxy_server(&proxy_path, proxy).await;

    // Request identities through proxy
    let filtered_identities = request_identities(&proxy_path).await;

    // Verify only work keys are returned
    assert_eq!(filtered_identities.len(), 2, "should have 2 work keys");
    assert!(
        filtered_identities
            .iter()
            .all(|i| i.comment.contains("@work"))
    );
}

#[tokio::test]
async fn test_proxy_filters_by_negation() {
    let temp_dir = TempDir::new().unwrap();
    let upstream_path = temp_dir.path().join("upstream.sock");
    let proxy_path = temp_dir.path().join("proxy.sock");

    // Create test identities
    let identities = vec![
        make_identity(ED25519_KEY_WORK),
        make_identity(ED25519_KEY_PERSONAL),
        make_identity(ED25519_KEY_DEV),
    ];

    // Start mock upstream agent
    start_mock_agent(&upstream_path, identities).await;

    // Create filter: exclude work keys
    let filter = FilterEvaluator::parse(&["not-comment=*@work*".to_string()]).unwrap();
    let upstream = Upstream::new(upstream_path.to_str().unwrap());
    let proxy = Arc::new(Proxy::new(upstream, filter));

    // Start proxy server
    start_proxy_server(&proxy_path, proxy).await;

    // Request identities through proxy
    let filtered_identities = request_identities(&proxy_path).await;

    // Verify only personal key is returned
    assert_eq!(filtered_identities.len(), 1, "should have 1 non-work key");
    assert_eq!(filtered_identities[0].comment, "user@personal.example.com");
}

#[tokio::test]
async fn test_proxy_filters_empty_allows_all() {
    let temp_dir = TempDir::new().unwrap();
    let upstream_path = temp_dir.path().join("upstream.sock");
    let proxy_path = temp_dir.path().join("proxy.sock");

    // Create test identities
    let identities = vec![
        make_identity(ED25519_KEY_WORK),
        make_identity(ED25519_KEY_PERSONAL),
        make_identity(ED25519_KEY_DEV),
    ];

    // Start mock upstream agent
    start_mock_agent(&upstream_path, identities).await;

    // Create empty filter (should allow all)
    let filter = FilterEvaluator::parse(&[]).unwrap();
    let upstream = Upstream::new(upstream_path.to_str().unwrap());
    let proxy = Arc::new(Proxy::new(upstream, filter));

    // Start proxy server
    start_proxy_server(&proxy_path, proxy).await;

    // Request identities through proxy
    let filtered_identities = request_identities(&proxy_path).await;

    // Verify all keys are returned
    assert_eq!(filtered_identities.len(), 3, "should have all 3 keys");
}

#[tokio::test]
async fn test_proxy_filters_multiple_rules() {
    let temp_dir = TempDir::new().unwrap();
    let upstream_path = temp_dir.path().join("upstream.sock");
    let proxy_path = temp_dir.path().join("proxy.sock");

    // Create test identities
    let identities = vec![
        make_identity(ED25519_KEY_WORK),
        make_identity(ED25519_KEY_PERSONAL),
        make_identity(ED25519_KEY_DEV),
    ];

    // Start mock upstream agent
    start_mock_agent(&upstream_path, identities).await;

    // Create filter: work keys but not dev
    let filter = FilterEvaluator::parse(&[
        "comment=*@work*".to_string(),
        "not-comment=dev@*".to_string(),
    ])
    .unwrap();
    let upstream = Upstream::new(upstream_path.to_str().unwrap());
    let proxy = Arc::new(Proxy::new(upstream, filter));

    // Start proxy server
    start_proxy_server(&proxy_path, proxy).await;

    // Request identities through proxy
    let filtered_identities = request_identities(&proxy_path).await;

    // Verify only user@work key is returned
    assert_eq!(filtered_identities.len(), 1, "should have 1 key");
    assert_eq!(filtered_identities[0].comment, "user@work.example.com");
}

#[tokio::test]
async fn test_proxy_filters_by_fingerprint() {
    let temp_dir = TempDir::new().unwrap();
    let upstream_path = temp_dir.path().join("upstream.sock");
    let proxy_path = temp_dir.path().join("proxy.sock");

    // Create test identities
    let work_identity = make_identity(ED25519_KEY_WORK);
    let fingerprint = work_identity.fingerprint().unwrap().to_string();

    let identities = vec![
        work_identity,
        make_identity(ED25519_KEY_PERSONAL),
        make_identity(ED25519_KEY_DEV),
    ];

    // Start mock upstream agent
    start_mock_agent(&upstream_path, identities).await;

    // Create filter: only allow specific fingerprint (auto-detected)
    let filter = FilterEvaluator::parse(&[fingerprint]).unwrap();
    let upstream = Upstream::new(upstream_path.to_str().unwrap());
    let proxy = Arc::new(Proxy::new(upstream, filter));

    // Start proxy server
    start_proxy_server(&proxy_path, proxy).await;

    // Request identities through proxy
    let filtered_identities = request_identities(&proxy_path).await;

    // Verify only the matched key is returned
    assert_eq!(filtered_identities.len(), 1, "should have 1 key");
    assert_eq!(filtered_identities[0].comment, "user@work.example.com");
}

#[tokio::test]
async fn test_proxy_filters_by_key_type() {
    let temp_dir = TempDir::new().unwrap();
    let upstream_path = temp_dir.path().join("upstream.sock");
    let proxy_path = temp_dir.path().join("proxy.sock");

    // Create test identities (all ed25519)
    let identities = vec![
        make_identity(ED25519_KEY_WORK),
        make_identity(ED25519_KEY_PERSONAL),
    ];

    // Start mock upstream agent
    start_mock_agent(&upstream_path, identities).await;

    // Create filter: only allow ed25519
    let filter = FilterEvaluator::parse(&["type=ed25519".to_string()]).unwrap();
    let upstream = Upstream::new(upstream_path.to_str().unwrap());
    let proxy = Arc::new(Proxy::new(upstream, filter));

    // Start proxy server
    start_proxy_server(&proxy_path, proxy).await;

    // Request identities through proxy
    let filtered_identities = request_identities(&proxy_path).await;

    // Verify all keys pass (they are all ed25519)
    assert_eq!(filtered_identities.len(), 2, "should have 2 ed25519 keys");
}

#[tokio::test]
async fn test_proxy_excludes_by_key_type() {
    let temp_dir = TempDir::new().unwrap();
    let upstream_path = temp_dir.path().join("upstream.sock");
    let proxy_path = temp_dir.path().join("proxy.sock");

    // Create test identities (all ed25519)
    let identities = vec![
        make_identity(ED25519_KEY_WORK),
        make_identity(ED25519_KEY_PERSONAL),
    ];

    // Start mock upstream agent
    start_mock_agent(&upstream_path, identities).await;

    // Create filter: exclude ed25519 (should return nothing)
    let filter = FilterEvaluator::parse(&["not-type=ed25519".to_string()]).unwrap();
    let upstream = Upstream::new(upstream_path.to_str().unwrap());
    let proxy = Arc::new(Proxy::new(upstream, filter));

    // Start proxy server
    start_proxy_server(&proxy_path, proxy).await;

    // Request identities through proxy
    let filtered_identities = request_identities(&proxy_path).await;

    // Verify no keys pass
    assert_eq!(
        filtered_identities.len(),
        0,
        "should have 0 keys (all excluded)"
    );
}
