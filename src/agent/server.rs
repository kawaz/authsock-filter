//! Unix socket server for SSH agent proxy
//!
//! This module provides a Unix socket server that listens for client
//! connections and spawns proxy handlers for each connection.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;
use tracing::{debug, error, info, trace, warn};

/// Unix socket server for accepting SSH agent client connections
pub struct Server {
    /// Path to the socket file
    socket_path: PathBuf,
    /// The listener (created on bind)
    listener: Option<UnixListener>,
}

impl Server {
    /// Create a new server that will listen on the specified path
    ///
    /// # Arguments
    /// * `socket_path` - Path where the Unix socket will be created
    pub fn new<P: AsRef<Path>>(socket_path: P) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
            listener: None,
        }
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Bind the server to the socket path
    ///
    /// This creates the Unix socket file. If a file already exists at the path,
    /// it will be removed first (to handle stale sockets from crashed processes).
    pub async fn bind(&mut self) -> Result<()> {
        // Remove existing socket if present
        if self.socket_path.exists() {
            debug!(path = %self.socket_path.display(), "Removing existing socket file");
            std::fs::remove_file(&self.socket_path).map_err(|e| {
                Error::Socket(format!(
                    "Failed to remove existing socket at {}: {}",
                    self.socket_path.display(),
                    e
                ))
            })?;
        }

        // Ensure parent directory exists
        if let Some(parent) = self.socket_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    Error::Socket(format!(
                        "Failed to create parent directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
        }

        // Create the listener
        let listener = UnixListener::bind(&self.socket_path).map_err(|e| {
            Error::Socket(format!(
                "Failed to bind to socket at {}: {}",
                self.socket_path.display(),
                e
            ))
        })?;

        info!(path = %self.socket_path.display(), "Server listening");
        self.listener = Some(listener);
        Ok(())
    }

    /// Accept the next client connection
    ///
    /// Returns `None` if the server is not bound or if the listener encounters a fatal error.
    pub async fn accept(&self) -> Result<UnixStream> {
        let listener = self.listener.as_ref().ok_or_else(|| {
            Error::Socket("Server is not bound".to_string())
        })?;

        let (stream, _addr) = listener.accept().await.map_err(|e| {
            Error::Socket(format!("Failed to accept connection: {}", e))
        })?;

        trace!("Accepted new client connection");
        Ok(stream)
    }

    /// Run the server with a connection handler
    ///
    /// This method runs until the shutdown signal is received.
    ///
    /// # Arguments
    /// * `handler` - Async function to handle each client connection
    /// * `shutdown_rx` - Watch receiver for shutdown signal
    pub async fn run<F, Fut>(
        &self,
        handler: F,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> Result<()>
    where
        F: Fn(UnixStream) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        let listener = self.listener.as_ref().ok_or_else(|| {
            Error::Socket("Server is not bound".to_string())
        })?;

        let handler = Arc::new(handler);

        loop {
            tokio::select! {
                // Handle shutdown signal
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Received shutdown signal, stopping server");
                        break;
                    }
                }

                // Accept new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            trace!("Accepted new client connection");
                            let handler = Arc::clone(&handler);
                            tokio::spawn(async move {
                                if let Err(e) = handler(stream).await {
                                    // Connection errors are expected (client disconnect, etc.)
                                    debug!(error = %e, "Connection handler error");
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to accept connection");
                            // Continue accepting other connections
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Clean up the socket file
    fn cleanup(&self) {
        if self.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                warn!(
                    path = %self.socket_path.display(),
                    error = %e,
                    "Failed to remove socket file during cleanup"
                );
            } else {
                debug!(path = %self.socket_path.display(), "Removed socket file");
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Guard that cleans up a socket path when dropped
pub struct SocketCleanupGuard {
    path: PathBuf,
}

impl SocketCleanupGuard {
    /// Create a new cleanup guard for the given socket path
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl Drop for SocketCleanupGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            if let Err(e) = std::fs::remove_file(&self.path) {
                warn!(
                    path = %self.path.display(),
                    error = %e,
                    "Failed to remove socket file during cleanup"
                );
            } else {
                debug!(path = %self.path.display(), "Removed socket file (guard)");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_server_new() {
        let server = Server::new("/tmp/test.sock");
        assert_eq!(server.socket_path(), Path::new("/tmp/test.sock"));
    }

    #[tokio::test]
    async fn test_server_bind_and_cleanup() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        {
            let mut server = Server::new(&socket_path);
            server.bind().await.unwrap();
            assert!(socket_path.exists());
        }

        // After drop, socket should be cleaned up
        assert!(!socket_path.exists());
    }

    #[tokio::test]
    async fn test_server_removes_stale_socket() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        // Create a stale file
        std::fs::write(&socket_path, b"stale").unwrap();
        assert!(socket_path.exists());

        let mut server = Server::new(&socket_path);
        server.bind().await.unwrap();

        // Should have replaced the stale file with a socket
        assert!(socket_path.exists());
    }

    #[test]
    fn test_socket_cleanup_guard() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("guard_test.sock");

        // Create a file
        std::fs::write(&socket_path, b"test").unwrap();
        assert!(socket_path.exists());

        {
            let _guard = SocketCleanupGuard::new(&socket_path);
        }

        // File should be removed after guard is dropped
        assert!(!socket_path.exists());
    }
}
