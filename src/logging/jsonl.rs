//! JSONL (JSON Lines) logging for structured log events
//!
//! This module provides JSONL format logging for SSH agent operations.
//! Each log entry is written as a single JSON object on one line.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

/// Log event kinds
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogEventKind {
    /// Server started
    ServerStart,
    /// Server stopped
    ServerStop,
    /// Client connected
    ClientConnect,
    /// Client disconnected
    ClientDisconnect,
    /// Identity list requested
    IdentitiesRequest,
    /// Identity list response
    IdentitiesResponse,
    /// Sign request
    SignRequest,
    /// Sign response (allowed or denied)
    SignResponse,
    /// Key filtered from list
    KeyFiltered,
    /// Key allowed in list
    KeyAllowed,
    /// Configuration loaded
    ConfigLoad,
    /// Configuration reload
    ConfigReload,
    /// Error occurred
    Error,
    /// SSH agent protocol message
    AgentMsg,
}

impl std::fmt::Display for LogEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogEventKind::ServerStart => write!(f, "server_start"),
            LogEventKind::ServerStop => write!(f, "server_stop"),
            LogEventKind::ClientConnect => write!(f, "client_connect"),
            LogEventKind::ClientDisconnect => write!(f, "client_disconnect"),
            LogEventKind::IdentitiesRequest => write!(f, "identities_request"),
            LogEventKind::IdentitiesResponse => write!(f, "identities_response"),
            LogEventKind::SignRequest => write!(f, "sign_request"),
            LogEventKind::SignResponse => write!(f, "sign_response"),
            LogEventKind::KeyFiltered => write!(f, "key_filtered"),
            LogEventKind::KeyAllowed => write!(f, "key_allowed"),
            LogEventKind::ConfigLoad => write!(f, "config_load"),
            LogEventKind::ConfigReload => write!(f, "config_reload"),
            LogEventKind::Error => write!(f, "error"),
            LogEventKind::AgentMsg => write!(f, "agent_msg"),
        }
    }
}

/// Decision result for sign requests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Request was allowed
    Allowed,
    /// Request was denied
    Denied,
}

/// Message direction for agent protocol logging
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageDirection {
    /// Request from client to agent
    Request,
    /// Response from agent to client
    Response,
}

/// Identity information for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityInfo {
    /// SSH key fingerprint
    pub fingerprint: String,
    /// Key comment
    pub comment: String,
    /// Key type (e.g., "ssh-ed25519")
    pub key_type: String,
}

/// SSH agent message content for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMsgContent {
    /// Message type number
    #[serde(rename = "type")]
    pub msg_type: u8,

    /// Message type name
    pub type_name: String,

    // IdentitiesAnswer fields
    /// List of identities (for IDENTITIES_ANSWER)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identities: Option<Vec<IdentityInfo>>,

    // SignRequest fields
    /// Key fingerprint (for SIGN_REQUEST)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,

    /// Data length to sign (for SIGN_REQUEST)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_len: Option<u32>,

    /// Signature flags (for SIGN_REQUEST)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u32>,

    // SignResponse fields
    /// Signature length (for SIGN_RESPONSE)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_len: Option<u32>,
}

impl AgentMsgContent {
    /// Create a new message content with just type info
    pub fn new(msg_type: u8, type_name: impl Into<String>) -> Self {
        Self {
            msg_type,
            type_name: type_name.into(),
            identities: None,
            fingerprint: None,
            data_len: None,
            flags: None,
            signature_len: None,
        }
    }

    /// Set identities answer fields
    pub fn with_identities(mut self, identities: Vec<IdentityInfo>) -> Self {
        self.identities = Some(identities);
        self
    }

    /// Set sign request fields
    pub fn with_sign_request(mut self, fingerprint: String, data_len: u32, flags: u32) -> Self {
        self.fingerprint = Some(fingerprint);
        self.data_len = Some(data_len);
        self.flags = Some(flags);
        self
    }

    /// Set sign response fields
    pub fn with_sign_response(mut self, signature_len: u32) -> Self {
        self.signature_len = Some(signature_len);
        self
    }
}

impl std::fmt::Display for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Decision::Allowed => write!(f, "allowed"),
            Decision::Denied => write!(f, "denied"),
        }
    }
}

/// A structured log event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Timestamp of the event
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: DateTime<Utc>,

    /// Kind of event
    pub kind: LogEventKind,

    /// Socket name (the filtered socket path)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket: Option<String>,

    /// Client identifier (connection ID or peer info)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// SSH key fingerprint (SHA256 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,

    /// SSH key comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// SSH key type (e.g., "ssh-ed25519", "ssh-rsa")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_type: Option<String>,

    /// Decision for sign requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,

    /// Reason for the decision or action
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Filter rule that matched
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_rule: Option<String>,

    /// Number of keys (for identity responses)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_count: Option<u32>,

    /// Number of keys filtered out
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filtered_count: Option<u32>,

    /// Error message (for error events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Additional context as key-value pairs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,

    /// Message direction (for agent_msg events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<MessageDirection>,

    /// Parsed message content (for agent_msg events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<AgentMsgContent>,

    /// Raw message data in base64 (for agent_msg events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_raw: Option<String>,

    /// Upstream socket path (for multi-upstream environments)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
}

impl LogEvent {
    /// Create a new log event with the current timestamp
    pub fn new(kind: LogEventKind) -> Self {
        Self {
            timestamp: Utc::now(),
            kind,
            socket: None,
            client_id: None,
            fingerprint: None,
            comment: None,
            key_type: None,
            decision: None,
            reason: None,
            matched_rule: None,
            key_count: None,
            filtered_count: None,
            error: None,
            context: None,
            direction: None,
            message: None,
            message_raw: None,
            upstream: None,
        }
    }

    /// Set the socket name
    pub fn with_socket(mut self, name: impl Into<String>) -> Self {
        self.socket = Some(name.into());
        self
    }

    /// Set the client ID
    pub fn with_client_id(mut self, id: impl Into<String>) -> Self {
        self.client_id = Some(id.into());
        self
    }

    /// Set the fingerprint
    pub fn with_fingerprint(mut self, fp: impl Into<String>) -> Self {
        self.fingerprint = Some(fp.into());
        self
    }

    /// Set the comment
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// Set the key type
    pub fn with_key_type(mut self, key_type: impl Into<String>) -> Self {
        self.key_type = Some(key_type.into());
        self
    }

    /// Set the decision
    pub fn with_decision(mut self, decision: Decision) -> Self {
        self.decision = Some(decision);
        self
    }

    /// Set the reason
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Set the matched rule
    pub fn with_matched_rule(mut self, rule: impl Into<String>) -> Self {
        self.matched_rule = Some(rule.into());
        self
    }

    /// Set the key count
    pub fn with_key_count(mut self, count: u32) -> Self {
        self.key_count = Some(count);
        self
    }

    /// Set the filtered count
    pub fn with_filtered_count(mut self, count: u32) -> Self {
        self.filtered_count = Some(count);
        self
    }

    /// Set the error message
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set additional context
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }

    /// Set the message direction
    pub fn with_direction(mut self, direction: MessageDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    /// Set the parsed message content
    pub fn with_message(mut self, message: AgentMsgContent) -> Self {
        self.message = Some(message);
        self
    }

    /// Set the raw message data (base64 encoded)
    pub fn with_message_raw(mut self, raw: impl Into<String>) -> Self {
        self.message_raw = Some(raw.into());
        self
    }

    /// Set the upstream socket path
    pub fn with_upstream(mut self, upstream: impl Into<String>) -> Self {
        self.upstream = Some(upstream.into());
        self
    }

    /// Create a server start event
    pub fn server_start(socket_path: impl Into<String>) -> Self {
        Self::new(LogEventKind::ServerStart).with_socket(socket_path)
    }

    /// Create a server stop event
    pub fn server_stop(socket_path: impl Into<String>) -> Self {
        Self::new(LogEventKind::ServerStop).with_socket(socket_path)
    }

    /// Create a client connect event
    pub fn client_connect(socket_path: impl Into<String>, client_id: impl Into<String>) -> Self {
        Self::new(LogEventKind::ClientConnect)
            .with_socket(socket_path)
            .with_client_id(client_id)
    }

    /// Create a client disconnect event
    pub fn client_disconnect(socket_path: impl Into<String>, client_id: impl Into<String>) -> Self {
        Self::new(LogEventKind::ClientDisconnect)
            .with_socket(socket_path)
            .with_client_id(client_id)
    }

    /// Create a sign request event
    pub fn sign_request(
        socket_path: impl Into<String>,
        fingerprint: impl Into<String>,
        comment: impl Into<String>,
    ) -> Self {
        Self::new(LogEventKind::SignRequest)
            .with_socket(socket_path)
            .with_fingerprint(fingerprint)
            .with_comment(comment)
    }

    /// Create a sign response event
    pub fn sign_response(
        socket_path: impl Into<String>,
        fingerprint: impl Into<String>,
        decision: Decision,
    ) -> Self {
        Self::new(LogEventKind::SignResponse)
            .with_socket(socket_path)
            .with_fingerprint(fingerprint)
            .with_decision(decision)
    }

    /// Create a key filtered event
    pub fn key_filtered(
        socket_path: impl Into<String>,
        fingerprint: impl Into<String>,
        comment: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::new(LogEventKind::KeyFiltered)
            .with_socket(socket_path)
            .with_fingerprint(fingerprint)
            .with_comment(comment)
            .with_reason(reason)
    }

    /// Create a key allowed event
    pub fn key_allowed(
        socket_path: impl Into<String>,
        fingerprint: impl Into<String>,
        comment: impl Into<String>,
    ) -> Self {
        Self::new(LogEventKind::KeyAllowed)
            .with_socket(socket_path)
            .with_fingerprint(fingerprint)
            .with_comment(comment)
    }

    /// Create an error event
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(LogEventKind::Error).with_error(message)
    }

    /// Create an agent message event
    pub fn agent_msg(direction: MessageDirection, message: AgentMsgContent) -> Self {
        Self::new(LogEventKind::AgentMsg)
            .with_direction(direction)
            .with_message(message)
    }

    /// Serialize the event to a JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// JSONL file writer with thread-safe buffered output
pub struct JsonlWriter {
    writer: Mutex<BufWriter<File>>,
}

impl JsonlWriter {
    /// Create a new JSONL writer
    ///
    /// Opens the file for appending. Creates the file if it doesn't exist.
    pub fn new<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;

        Ok(Self {
            writer: Mutex::new(BufWriter::new(file)),
        })
    }

    /// Write a log event to the file
    pub fn write(&self, event: &LogEvent) -> std::io::Result<()> {
        let json = event
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| std::io::Error::other("Lock poisoned"))?;

        writeln!(writer, "{}", json)?;
        writer.flush()?;

        Ok(())
    }

    /// Flush any buffered data to the file
    pub fn flush(&self) -> std::io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| std::io::Error::other("Lock poisoned"))?;

        writer.flush()
    }
}

impl Drop for JsonlWriter {
    fn drop(&mut self) {
        // Best effort flush on drop
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use tempfile::NamedTempFile;

    #[test]
    fn test_log_event_new() {
        let event = LogEvent::new(LogEventKind::ServerStart);
        assert_eq!(event.kind, LogEventKind::ServerStart);
        assert!(event.socket.is_none());
    }

    #[test]
    fn test_log_event_builder() {
        let event = LogEvent::new(LogEventKind::SignRequest)
            .with_socket("/tmp/test.sock")
            .with_fingerprint("SHA256:abc123")
            .with_comment("test@example.com")
            .with_key_type("ssh-ed25519");

        assert_eq!(event.kind, LogEventKind::SignRequest);
        assert_eq!(event.socket, Some("/tmp/test.sock".to_string()));
        assert_eq!(event.fingerprint, Some("SHA256:abc123".to_string()));
        assert_eq!(event.comment, Some("test@example.com".to_string()));
        assert_eq!(event.key_type, Some("ssh-ed25519".to_string()));
    }

    #[test]
    fn test_log_event_serialize() {
        let event = LogEvent::server_start("/tmp/test.sock");
        let json = event.to_json().unwrap();

        assert!(json.contains("\"kind\":\"server_start\""));
        assert!(json.contains("\"socket\":\"/tmp/test.sock\""));
        assert!(json.contains("\"timestamp\":"));
    }

    #[test]
    fn test_log_event_sign_response() {
        let event = LogEvent::sign_response("/tmp/test.sock", "SHA256:abc", Decision::Allowed);

        assert_eq!(event.kind, LogEventKind::SignResponse);
        assert_eq!(event.decision, Some(Decision::Allowed));
    }

    #[test]
    fn test_jsonl_writer() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();

        {
            let writer = JsonlWriter::new(&path).unwrap();
            writer
                .write(&LogEvent::server_start("/tmp/test.sock"))
                .unwrap();
            writer
                .write(&LogEvent::client_connect("/tmp/test.sock", "client-1"))
                .unwrap();
        }

        // Read back and verify
        let file = File::open(&path).unwrap();
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().map(|l| l.unwrap()).collect();

        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"kind\":\"server_start\""));
        assert!(lines[1].contains("\"kind\":\"client_connect\""));
    }

    #[test]
    fn test_log_event_kind_display() {
        assert_eq!(LogEventKind::ServerStart.to_string(), "server_start");
        assert_eq!(LogEventKind::SignRequest.to_string(), "sign_request");
        assert_eq!(LogEventKind::KeyFiltered.to_string(), "key_filtered");
    }

    #[test]
    fn test_decision_display() {
        assert_eq!(Decision::Allowed.to_string(), "allowed");
        assert_eq!(Decision::Denied.to_string(), "denied");
    }

    #[test]
    fn test_log_event_deserialize() {
        let event = LogEvent::sign_response("/tmp/test.sock", "SHA256:abc", Decision::Denied)
            .with_reason("No matching allow rule");

        let json = event.to_json().unwrap();
        let parsed: LogEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.kind, LogEventKind::SignResponse);
        assert_eq!(parsed.decision, Some(Decision::Denied));
        assert_eq!(parsed.reason, Some("No matching allow rule".to_string()));
    }
}
