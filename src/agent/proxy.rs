//! SSH Agent proxy core logic
//!
//! This module implements the core proxy functionality that filters
//! SSH agent requests between a client and the upstream agent.

use crate::error::{Error, Result};
use crate::filter::FilterEvaluator;
use crate::protocol::{AgentCodec, AgentMessage, Identity, MessageType};
use std::collections::HashSet;
use std::sync::Arc;
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
    /// Cached set of allowed key blobs (key_blob bytes as key)
    allowed_keys: Arc<RwLock<HashSet<Vec<u8>>>>,
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
            allowed_keys: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Create a new proxy with Arc-wrapped components
    pub fn new_shared(upstream: Arc<Upstream>, filter: Arc<FilterEvaluator>) -> Self {
        Self {
            upstream,
            filter,
            allowed_keys: Arc::new(RwLock::new(HashSet::new())),
        }
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

            // Process the request
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

        // Update allowed keys cache
        {
            let mut allowed = self.allowed_keys.write().await;
            allowed.clear();
            for identity in &filtered {
                allowed.insert(identity.key_blob.to_vec());
            }
        }

        // Build filtered response
        Ok(AgentMessage::build_identities_answer(&filtered))
    }

    /// Handle SSH_AGENTC_SIGN_REQUEST (13)
    ///
    /// Only allows signing with keys that are in the allowed set
    /// (i.e., keys that passed the filter in a previous REQUEST_IDENTITIES).
    async fn handle_sign_request(&self, request: AgentMessage) -> Result<AgentMessage> {
        debug!("Handling SIGN_REQUEST");

        // Parse the key blob from the request
        let key_blob = match request.parse_sign_request_key() {
            Ok(blob) => blob,
            Err(e) => {
                warn!(error = %e, "Failed to parse sign request");
                return Ok(AgentMessage::failure());
            }
        };

        // Check if this key is allowed
        let allowed = self.allowed_keys.read().await;
        if !allowed.contains(key_blob.as_ref()) {
            // Key not allowed, try to create identity and check filter
            // This handles the case where a client requests signing without
            // first requesting identities
            let identity = Identity::new(key_blob.clone(), String::new());
            if !self.filter.matches(&identity) {
                info!(
                    fingerprint = ?identity.fingerprint(),
                    "Sign request denied: key not allowed by filter"
                );
                return Ok(AgentMessage::failure());
            }
        }
        drop(allowed);

        // Key is allowed, forward to upstream
        let response = self.forward_to_upstream(request).await?;

        if response.msg_type == MessageType::SignResponse {
            let identity = Identity::new(key_blob, String::new());
            debug!(
                fingerprint = ?identity.fingerprint(),
                "Sign request succeeded"
            );
        }

        Ok(response)
    }

    /// Forward a message to the upstream agent
    async fn forward_to_upstream(&self, request: AgentMessage) -> Result<AgentMessage> {
        let mut conn = self.upstream.connect().await?;
        conn.send_receive(&request).await
    }
}

/// Builder for creating a Proxy with optional configuration
pub struct ProxyBuilder {
    upstream: Option<Upstream>,
    filter: FilterEvaluator,
}

impl ProxyBuilder {
    /// Create a new proxy builder
    pub fn new() -> Self {
        Self {
            upstream: None,
            filter: FilterEvaluator::default(),
        }
    }

    /// Set the upstream agent connection
    pub fn upstream(mut self, upstream: Upstream) -> Self {
        self.upstream = Some(upstream);
        self
    }

    /// Set the upstream from SSH_AUTH_SOCK
    pub fn upstream_from_env(mut self) -> Result<Self> {
        self.upstream = Some(Upstream::from_env()?);
        Ok(self)
    }

    /// Set the filter evaluator
    pub fn filter(mut self, filter: FilterEvaluator) -> Self {
        self.filter = filter;
        self
    }

    /// Parse filter strings and set the evaluator
    pub fn filter_strs(mut self, filter_strs: &[String]) -> Result<Self> {
        self.filter = FilterEvaluator::parse(filter_strs)?;
        Ok(self)
    }

    /// Build the proxy
    pub fn build(self) -> Result<Proxy> {
        let upstream = self
            .upstream
            .ok_or_else(|| Error::Config("Upstream agent not configured".to_string()))?;

        Ok(Proxy::new(upstream, self.filter))
    }
}

impl Default for ProxyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_builder() {
        let upstream = Upstream::new("/tmp/test.sock");
        let filter = FilterEvaluator::default();

        let proxy = ProxyBuilder::new()
            .upstream(upstream)
            .filter(filter)
            .build()
            .unwrap();

        assert!(proxy.filter().is_empty());
    }

    #[test]
    fn test_proxy_builder_missing_upstream() {
        let result = ProxyBuilder::new().build();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_allowed_keys_cache() {
        let upstream = Upstream::new("/tmp/test.sock");
        let filter = FilterEvaluator::default();
        let proxy = Proxy::new(upstream, filter);

        // Initially empty
        assert!(proxy.allowed_keys.read().await.is_empty());

        // Add a key
        {
            let mut allowed = proxy.allowed_keys.write().await;
            allowed.insert(vec![1, 2, 3]);
        }

        // Should contain the key
        assert!(proxy.allowed_keys.read().await.contains(&vec![1, 2, 3]));
    }
}
