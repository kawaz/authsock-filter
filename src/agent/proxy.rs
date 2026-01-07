//! SSH Agent proxy core logic
//!
//! This module implements the core proxy functionality that filters
//! SSH agent requests between a client and the upstream agent.

use crate::error::Result;
use crate::filter::FilterEvaluator;
use crate::protocol::{AgentCodec, AgentMessage, Identity, MessageType};
use bytes::Bytes;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::UnixStream;
use tokio::sync::RwLock;
use tracing::{debug, info, trace, warn};

use super::Upstream;

/// SSH Agent proxy that filters requests
pub struct Proxy {
    /// Upstream agent connection manager
    upstream: Arc<Upstream>,
    /// Filter evaluator for key filtering
    filter: Arc<FilterEvaluator>,
    /// Socket path for identification
    socket_path: String,
    /// Connection counter for client IDs
    connection_counter: AtomicU64,
    /// Socket-level cache for allowed keys (shared across all connections)
    /// Updated when REQUEST_IDENTITIES is processed
    allowed_keys_cache: Arc<RwLock<HashSet<Bytes>>>,
}

impl Proxy {
    /// Create a new proxy
    ///
    /// # Arguments
    /// * `upstream` - Upstream agent connection manager
    /// * `filter` - Filter evaluator for key filtering
    pub fn new(upstream: Upstream, filter: FilterEvaluator) -> Self {
        Self {
            upstream: Arc::new(upstream),
            filter: Arc::new(filter),
            socket_path: String::new(),
            connection_counter: AtomicU64::new(0),
            allowed_keys_cache: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Create a new proxy with Arc-wrapped components
    pub fn new_shared(upstream: Arc<Upstream>, filter: Arc<FilterEvaluator>) -> Self {
        Self {
            upstream,
            filter,
            socket_path: String::new(),
            connection_counter: AtomicU64::new(0),
            allowed_keys_cache: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Set the socket path for identification
    pub fn with_socket_path(mut self, path: impl Into<String>) -> Self {
        self.socket_path = path.into();
        self
    }

    /// Get a reference to the upstream
    pub fn upstream(&self) -> &Upstream {
        &self.upstream
    }

    /// Get a reference to the filter
    pub fn filter(&self) -> &FilterEvaluator {
        &self.filter
    }

    /// Handle a client connection
    ///
    /// This method processes messages from the client, applies filtering,
    /// and forwards requests to the upstream agent.
    pub async fn handle_client(&self, mut client_stream: UnixStream) -> Result<()> {
        let client_id = self.connection_counter.fetch_add(1, Ordering::Relaxed);
        debug!(
            socket = %self.socket_path,
            client_id = client_id,
            "Client connected"
        );

        let result = self.handle_client_inner(&mut client_stream).await;

        debug!(
            socket = %self.socket_path,
            client_id = client_id,
            "Client disconnected"
        );

        result
    }

    async fn handle_client_inner(&self, client_stream: &mut UnixStream) -> Result<()> {
        let (mut client_reader, mut client_writer) = client_stream.split();

        loop {
            // Read request from client
            let request = match AgentCodec::read(&mut client_reader).await? {
                Some(msg) => msg,
                None => {
                    trace!("Client disconnected");
                    break;
                }
            };

            trace!(msg_type = ?request.msg_type, "Received request from client");

            // Process the request (uses socket-level shared cache)
            let response = self.process_request(request).await?;

            // Send response to client
            AgentCodec::write(&mut client_writer, &response).await?;
        }

        Ok(())
    }

    /// Process a single request from the client
    async fn process_request(&self, request: AgentMessage) -> Result<AgentMessage> {
        match request.msg_type {
            MessageType::RequestIdentities => self.handle_request_identities(request).await,
            MessageType::SignRequest => self.handle_sign_request(request).await,
            _ => {
                // Pass through other messages
                self.forward_to_upstream(request).await
            }
        }
    }

    /// Handle SSH_AGENTC_REQUEST_IDENTITIES (11)
    ///
    /// Forwards the request to upstream, then filters the response
    /// to only include keys that match the filter rules.
    async fn handle_request_identities(&self, request: AgentMessage) -> Result<AgentMessage> {
        debug!("Handling REQUEST_IDENTITIES");

        // Forward to upstream
        let response = self.forward_to_upstream(request).await?;

        // Only process if we got an IdentitiesAnswer
        if response.msg_type != MessageType::IdentitiesAnswer {
            warn!(msg_type = ?response.msg_type, "Unexpected response type for REQUEST_IDENTITIES");
            return Ok(response);
        }

        // Parse the identities
        let identities = match response.parse_identities() {
            Ok(ids) => ids,
            Err(e) => {
                warn!(error = %e, "Failed to parse identities from upstream");
                return Ok(AgentMessage::failure());
            }
        };

        let original_count = identities.len();
        debug!(count = original_count, "Received identities from upstream");

        // Filter the identities
        let filtered: Vec<Identity> = identities
            .into_iter()
            .filter(|id| self.filter.matches(id))
            .collect();

        let filtered_count = filtered.len();
        info!(
            original = original_count,
            filtered = filtered_count,
            "Filtered identities"
        );

        // Update socket-level shared allowed keys cache
        {
            let mut cache = self.allowed_keys_cache.write().await;
            cache.clear();
            for identity in &filtered {
                cache.insert(identity.key_blob.clone());
            }
        }

        // Build filtered response
        Ok(AgentMessage::build_identities_answer(&filtered))
    }

    /// Handle SSH_AGENTC_SIGN_REQUEST (13)
    ///
    /// Only allows signing with keys that are in the allowed set
    /// (i.e., keys that passed the filter in a previous REQUEST_IDENTITIES),
    /// or keys that match the filter directly.
    async fn handle_sign_request(&self, request: AgentMessage) -> Result<AgentMessage> {
        // Parse the key blob from the request
        let key_blob = match request.parse_sign_request_key() {
            Ok(blob) => blob,
            Err(e) => {
                warn!(error = %e, "Failed to parse sign request");
                return Ok(AgentMessage::failure());
            }
        };

        // Create identity for filter check
        let identity = Identity::new(key_blob.clone(), String::new());

        // Check if this key is allowed:
        // 1. First check the socket-level shared cache (from REQUEST_IDENTITIES)
        // 2. If not in cache, apply the filter directly
        //    (handles cases where SSH client uses separate connections)
        let is_allowed = {
            let cache = self.allowed_keys_cache.read().await;
            cache.contains(key_blob.as_ref())
        } || self.filter.matches(&identity);
        // Note: filter.matches() is called outside the lock to avoid holding
        // the lock longer than necessary. This also handles cases where
        // SSH client uses separate connections and the key wasn't cached.
        // For comment-based filters, this may not match, but that's acceptable
        // since the key should have been in the cache from REQUEST_IDENTITIES.

        if !is_allowed {
            warn!("Sign request denied: key not allowed by filter");
            return Ok(AgentMessage::failure());
        }

        // Forward to upstream
        self.forward_to_upstream(request).await
    }

    /// Forward a message to the upstream agent
    async fn forward_to_upstream(&self, request: AgentMessage) -> Result<AgentMessage> {
        let mut conn = self.upstream.connect().await?;
        conn.send_receive(&request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_creation() {
        let upstream = Upstream::new("/tmp/test.sock");
        let filter = FilterEvaluator::default();
        let proxy = Proxy::new(upstream, filter);

        // Proxy should be created successfully
        assert_eq!(proxy.socket_path, "");
    }

    #[test]
    fn test_proxy_with_socket_path() {
        let upstream = Upstream::new("/tmp/test.sock");
        let filter = FilterEvaluator::default();
        let proxy = Proxy::new(upstream, filter).with_socket_path("/tmp/my.sock");

        assert_eq!(proxy.socket_path, "/tmp/my.sock");
    }
}
