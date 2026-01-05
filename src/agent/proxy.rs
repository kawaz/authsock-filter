//! SSH Agent proxy core logic
//!
//! This module implements the core proxy functionality that filters
//! SSH agent requests between a client and the upstream agent.

use crate::error::Result;
use crate::filter::FilterEvaluator;
use crate::logging::jsonl::{
    AgentMsgContent, Decision, IdentityInfo, JsonlWriter, LogEvent, MessageDirection,
};
use crate::protocol::{AgentCodec, AgentMessage, Identity, MessageType};
use base64::Engine;
use bytes::Buf;
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
    /// Cached set of allowed key blobs (key_blob bytes as key)
    allowed_keys: Arc<RwLock<HashSet<Vec<u8>>>>,
    /// Socket path for logging
    socket_path: String,
    /// JSONL logger (optional)
    logger: Option<Arc<JsonlWriter>>,
    /// Connection counter for client IDs
    connection_counter: AtomicU64,
    /// Verbosity level for agent message logging
    /// 0: no agent_msg, 1: message only, 2+: message + message_raw
    verbosity: i8,
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
            socket_path: String::new(),
            logger: None,
            connection_counter: AtomicU64::new(0),
            verbosity: 0,
        }
    }

    /// Create a new proxy with Arc-wrapped components
    pub fn new_shared(upstream: Arc<Upstream>, filter: Arc<FilterEvaluator>) -> Self {
        Self {
            upstream,
            filter,
            allowed_keys: Arc::new(RwLock::new(HashSet::new())),
            socket_path: String::new(),
            logger: None,
            connection_counter: AtomicU64::new(0),
            verbosity: 0,
        }
    }

    /// Set the socket path for logging
    pub fn with_socket_path(mut self, path: impl Into<String>) -> Self {
        self.socket_path = path.into();
        self
    }

    /// Set the JSONL logger
    pub fn with_logger(mut self, logger: Arc<JsonlWriter>) -> Self {
        self.logger = Some(logger);
        self
    }

    /// Set the verbosity level for agent message logging
    /// 0: no agent_msg, 1: message only, 2+: message + message_raw
    pub fn with_verbosity(mut self, verbosity: i8) -> Self {
        self.verbosity = verbosity;
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

    /// Log an event if logger is configured
    fn log(&self, event: LogEvent) {
        if let Some(ref logger) = self.logger
            && let Err(e) = logger.write(&event)
        {
            warn!(error = %e, "Failed to write log event");
        }
    }

    /// Log an agent message
    /// Only logs if verbosity >= 1 (DEBUG level)
    /// Includes message_raw only if verbosity >= 2 (TRACE level)
    fn log_agent_msg(&self, msg: &AgentMessage, direction: MessageDirection, client_id: &str) {
        // verbosity 0: no agent_msg logging
        if self.verbosity < 1 {
            return;
        }

        let msg_type_byte: u8 = msg.msg_type.into();
        let msg_name = msg.msg_type.as_str();

        // Build parsed message content based on message type
        let message_content = self.parse_message_content(msg, msg_type_byte, msg_name);

        let upstream_path = self.upstream.socket_path().to_string_lossy().to_string();

        let mut event = LogEvent::agent_msg(direction, message_content)
            .with_socket(&self.socket_path)
            .with_client_id(client_id)
            .with_upstream(&upstream_path);

        // verbosity >= 2: include message_raw (TRACE level)
        if self.verbosity >= 2 {
            let mut raw_bytes = vec![msg_type_byte];
            raw_bytes.extend_from_slice(&msg.payload);
            let raw = base64::engine::general_purpose::STANDARD.encode(&raw_bytes);
            event = event.with_message_raw(raw);
        }

        self.log(event);
    }

    /// Parse message content for logging based on message type
    fn parse_message_content(
        &self,
        msg: &AgentMessage,
        msg_type: u8,
        msg_name: &str,
    ) -> AgentMsgContent {
        let base = AgentMsgContent::new(msg_type, msg_name);

        match msg.msg_type {
            MessageType::IdentitiesAnswer => {
                // Parse identities from response
                if let Ok(identities) = msg.parse_identities() {
                    let identity_infos: Vec<IdentityInfo> = identities
                        .iter()
                        .map(|id| IdentityInfo {
                            fingerprint: id
                                .fingerprint()
                                .map(|f| f.to_string())
                                .unwrap_or_default(),
                            comment: id.comment.clone(),
                            key_type: id.key_type().unwrap_or_default(),
                        })
                        .collect();
                    base.with_identities(identity_infos)
                } else {
                    base
                }
            }
            MessageType::SignRequest => {
                // Parse sign request fields
                let mut buf = &msg.payload[..];
                if buf.remaining() >= 4 {
                    let key_len = buf.get_u32() as usize;
                    if buf.remaining() >= key_len {
                        let key_blob = &buf[..key_len];
                        buf.advance(key_len);

                        // Get fingerprint from key blob
                        let identity =
                            Identity::new(bytes::Bytes::copy_from_slice(key_blob), String::new());
                        let fingerprint = identity
                            .fingerprint()
                            .map(|f| f.to_string())
                            .unwrap_or_default();

                        // Parse data length
                        let data_len = if buf.remaining() >= 4 {
                            let len = buf.get_u32();
                            buf.advance(len as usize);
                            len
                        } else {
                            0
                        };

                        // Parse flags
                        let flags = if buf.remaining() >= 4 {
                            buf.get_u32()
                        } else {
                            0
                        };

                        return base.with_sign_request(fingerprint, data_len, flags);
                    }
                }
                base
            }
            MessageType::SignResponse => {
                // Parse signature length
                let mut buf = &msg.payload[..];
                if buf.remaining() >= 4 {
                    let sig_len = buf.get_u32();
                    base.with_sign_response(sig_len)
                } else {
                    base
                }
            }
            // For other message types, just return the base info
            _ => base,
        }
    }

    /// Handle a client connection
    ///
    /// This method processes messages from the client, applies filtering,
    /// and forwards requests to the upstream agent.
    pub async fn handle_client(&self, mut client_stream: UnixStream) -> Result<()> {
        let client_id = self.connection_counter.fetch_add(1, Ordering::SeqCst);
        let client_id_str = format!("conn-{}", client_id);

        self.log(LogEvent::client_connect(&self.socket_path, &client_id_str));

        let result = self
            .handle_client_inner(&mut client_stream, &client_id_str)
            .await;

        self.log(LogEvent::client_disconnect(
            &self.socket_path,
            &client_id_str,
        ));

        result
    }

    async fn handle_client_inner(
        &self,
        client_stream: &mut UnixStream,
        client_id: &str,
    ) -> Result<()> {
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
            let response = self.process_request(request, client_id).await?;

            // Send response to client
            AgentCodec::write(&mut client_writer, &response).await?;
        }

        Ok(())
    }

    /// Process a single request from the client
    async fn process_request(
        &self,
        request: AgentMessage,
        client_id: &str,
    ) -> Result<AgentMessage> {
        match request.msg_type {
            MessageType::RequestIdentities => {
                self.handle_request_identities(request, client_id).await
            }
            MessageType::SignRequest => self.handle_sign_request(request, client_id).await,
            _ => {
                // Pass through other messages
                self.forward_to_upstream(request, client_id).await
            }
        }
    }

    /// Handle SSH_AGENTC_REQUEST_IDENTITIES (11)
    ///
    /// Forwards the request to upstream, then filters the response
    /// to only include keys that match the filter rules.
    async fn handle_request_identities(
        &self,
        request: AgentMessage,
        client_id: &str,
    ) -> Result<AgentMessage> {
        debug!("Handling REQUEST_IDENTITIES");

        // Forward to upstream
        let response = self.forward_to_upstream(request, client_id).await?;

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

        // Filter the identities and log each one
        let mut filtered: Vec<Identity> = Vec::new();
        for id in identities {
            let fingerprint = id.fingerprint().map(|f| f.to_string()).unwrap_or_default();
            let key_type = id.key_type().unwrap_or_default();

            if self.filter.matches(&id) {
                // Log key allowed
                self.log(
                    LogEvent::key_allowed(&self.socket_path, &fingerprint, &id.comment)
                        .with_key_type(&key_type)
                        .with_client_id(client_id),
                );
                filtered.push(id);
            } else {
                // Log key filtered
                self.log(
                    LogEvent::key_filtered(
                        &self.socket_path,
                        &fingerprint,
                        &id.comment,
                        "no matching rule",
                    )
                    .with_key_type(&key_type)
                    .with_client_id(client_id),
                );
            }
        }

        let filtered_count = filtered.len();
        info!(
            original = original_count,
            filtered = filtered_count,
            "Filtered identities"
        );

        // Log identities response summary
        self.log(
            LogEvent::new(crate::logging::jsonl::LogEventKind::IdentitiesResponse)
                .with_socket(&self.socket_path)
                .with_client_id(client_id)
                .with_key_count(filtered_count as u32)
                .with_filtered_count((original_count - filtered_count) as u32),
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
    async fn handle_sign_request(
        &self,
        request: AgentMessage,
        client_id: &str,
    ) -> Result<AgentMessage> {
        // Parse the key blob from the request
        let key_blob = match request.parse_sign_request_key() {
            Ok(blob) => blob,
            Err(e) => {
                warn!(error = %e, "Failed to parse sign request");
                return Ok(AgentMessage::failure());
            }
        };

        // Get fingerprint for logging
        let identity = Identity::new(key_blob.clone(), String::new());
        let fingerprint = identity
            .fingerprint()
            .map(|f| f.to_string())
            .unwrap_or_default();

        // Log sign request
        self.log(
            LogEvent::new(crate::logging::jsonl::LogEventKind::SignRequest)
                .with_socket(&self.socket_path)
                .with_client_id(client_id)
                .with_fingerprint(&fingerprint),
        );

        // Check if this key is in the allowed set
        let allowed = self.allowed_keys.read().await;
        if !allowed.contains(key_blob.as_ref()) {
            debug!("Sign request denied: key not in allowed set");
            self.log(
                LogEvent::sign_response(&self.socket_path, &fingerprint, Decision::Denied)
                    .with_client_id(client_id)
                    .with_reason("key not in allowed set"),
            );
            return Ok(AgentMessage::failure());
        }
        drop(allowed);

        // Forward to upstream
        let response = self.forward_to_upstream(request, client_id).await?;

        // Log result
        let decision = if response.msg_type == MessageType::SignResponse {
            Decision::Allowed
        } else {
            Decision::Denied
        };
        self.log(
            LogEvent::sign_response(&self.socket_path, &fingerprint, decision)
                .with_client_id(client_id),
        );

        Ok(response)
    }

    /// Forward a message to the upstream agent
    async fn forward_to_upstream(
        &self,
        request: AgentMessage,
        client_id: &str,
    ) -> Result<AgentMessage> {
        // Log request
        self.log_agent_msg(&request, MessageDirection::Request, client_id);

        let mut conn = self.upstream.connect().await?;
        let response = conn.send_receive(&request).await?;

        // Log response
        self.log_agent_msg(&response, MessageDirection::Response, client_id);

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
