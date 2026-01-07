//! Upstream SSH agent connection
//!
//! This module handles the connection to the upstream SSH agent,
//! typically accessed via the SSH_AUTH_SOCK environment variable.

use crate::error::{Error, Result};
use crate::protocol::{AgentCodec, AgentMessage};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::net::UnixStream;
use tracing::{debug, trace};

/// Default connection timeout for upstream agent
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Connection to an upstream SSH agent
pub struct Upstream {
    /// Path to the upstream agent socket
    socket_path: PathBuf,
}

impl Upstream {
    /// Create a new upstream connection manager
    ///
    /// # Arguments
    /// * `socket_path` - Path to the upstream agent socket
    pub fn new<P: AsRef<Path>>(socket_path: P) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
        }
    }

    /// Create from SSH_AUTH_SOCK environment variable
    pub fn from_env() -> Result<Self> {
        let socket_path = std::env::var("SSH_AUTH_SOCK").map_err(|_| {
            Error::UpstreamNotAvailable("SSH_AUTH_SOCK environment variable not set".to_string())
        })?;

        let path = PathBuf::from(&socket_path);
        if !path.exists() {
            return Err(Error::UpstreamNotAvailable(format!(
                "SSH agent socket does not exist: {}",
                socket_path
            )));
        }

        debug!(socket_path = %socket_path, "Using upstream agent from SSH_AUTH_SOCK");
        Ok(Self::new(path))
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Connect to the upstream agent with timeout
    pub async fn connect(&self) -> Result<UpstreamConnection> {
        let stream = tokio::time::timeout(
            DEFAULT_CONNECT_TIMEOUT,
            UnixStream::connect(&self.socket_path),
        )
        .await
        .map_err(|_| {
            Error::UpstreamNotAvailable(format!(
                "Connection to upstream agent at {} timed out after {:?}",
                self.socket_path.display(),
                DEFAULT_CONNECT_TIMEOUT
            ))
        })?
        .map_err(|e| {
            Error::UpstreamNotAvailable(format!(
                "Failed to connect to upstream agent at {}: {}",
                self.socket_path.display(),
                e
            ))
        })?;

        trace!(socket_path = %self.socket_path.display(), "Connected to upstream agent");
        Ok(UpstreamConnection { stream })
    }
}

/// An active connection to the upstream agent
#[derive(Debug)]
pub struct UpstreamConnection {
    stream: UnixStream,
}

impl UpstreamConnection {
    /// Send a message to the upstream agent and receive the response
    pub async fn send_receive(&mut self, msg: &AgentMessage) -> Result<AgentMessage> {
        trace!(msg_type = ?msg.msg_type, "Sending message to upstream");

        let (mut reader, mut writer) = self.stream.split();

        // Send the message
        AgentCodec::write(&mut writer, msg).await?;

        // Read the response
        let response = AgentCodec::read(&mut reader).await?.ok_or_else(|| {
            Error::Protocol("Upstream agent closed connection unexpectedly".to_string())
        })?;

        trace!(response_type = ?response.msg_type, "Received response from upstream");
        Ok(response)
    }

    /// Get mutable access to the underlying stream for split operations
    pub fn stream_mut(&mut self) -> &mut UnixStream {
        &mut self.stream
    }

    /// Consume self and return the underlying stream
    pub fn into_stream(self) -> UnixStream {
        self.stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upstream_new() {
        let upstream = Upstream::new("/tmp/test.sock");
        assert_eq!(upstream.socket_path(), Path::new("/tmp/test.sock"));
    }

    #[tokio::test]
    async fn test_upstream_from_env_missing() {
        // Clear SSH_AUTH_SOCK to test missing case
        // SAFETY: This test runs in isolation
        unsafe { std::env::remove_var("SSH_AUTH_SOCK") };
        let result = Upstream::from_env();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_connect_nonexistent_socket() {
        // Attempt to connect to a socket that doesn't exist
        let upstream = Upstream::new("/tmp/nonexistent-socket-12345.sock");
        let result = upstream.connect().await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to connect") || err.contains("upstream"));
    }

    #[tokio::test]
    async fn test_connect_not_a_socket() {
        // Attempt to connect to a regular file (not a socket)
        let temp_file = std::env::temp_dir().join("not-a-socket-test.txt");
        std::fs::write(&temp_file, "test").unwrap();
        let upstream = Upstream::new(&temp_file);
        let result = upstream.connect().await;
        std::fs::remove_file(&temp_file).ok();
        assert!(result.is_err());
    }
}
