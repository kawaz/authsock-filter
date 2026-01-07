//! SSH Agent protocol codec for tokio

use crate::error::{Error, Result};
use crate::protocol::message::AgentMessage;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum message size (16MB, same as OpenSSH)
const MAX_MESSAGE_SIZE: u32 = 16 * 1024 * 1024;

/// Codec for reading and writing SSH agent messages
pub struct AgentCodec;

impl AgentCodec {
    /// Read a message from an async reader
    pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Option<AgentMessage>> {
        // Read length prefix (4 bytes)
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }

        let len = u32::from_be_bytes(len_buf);
        if len == 0 {
            return Err(Error::InvalidMessage("Zero-length message".to_string()));
        }
        if len > MAX_MESSAGE_SIZE {
            return Err(Error::InvalidMessage(format!(
                "Message too large: {} bytes",
                len
            )));
        }

        // Read message body
        let mut buf = vec![0u8; len as usize];
        reader.read_exact(&mut buf).await?;

        let msg = AgentMessage::decode(&buf)?;
        Ok(Some(msg))
    }

    /// Write a message to an async writer
    pub async fn write<W: AsyncWrite + Unpin>(writer: &mut W, msg: &AgentMessage) -> Result<()> {
        let encoded = msg.encode();
        writer.write_all(&encoded).await?;
        writer.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_read_zero_length_message() {
        // Zero-length message should error
        let mut data = Cursor::new(vec![0u8, 0, 0, 0]);
        let result = AgentCodec::read(&mut data).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Zero-length"));
    }

    #[tokio::test]
    async fn test_read_message_too_large() {
        // Message size exceeding MAX_MESSAGE_SIZE (16MB)
        let mut data = Cursor::new(vec![0x01, 0x00, 0x00, 0x01]); // 16MB + 1
        let result = AgentCodec::read(&mut data).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[tokio::test]
    async fn test_read_eof() {
        // Empty input should return None
        let mut data = Cursor::new(vec![]);
        let result = AgentCodec::read(&mut data).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_read_truncated_length() {
        // Partial length prefix (only 2 bytes) - returns error due to UnexpectedEof
        let mut data = Cursor::new(vec![0u8, 0]);
        let result = AgentCodec::read(&mut data).await;
        // This returns Ok(None) because read_exact fails with UnexpectedEof
        // which is handled specially to return None (connection closed)
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_read_truncated_body() {
        // Length says 10 bytes but only 5 available
        let mut data = Cursor::new(vec![0u8, 0, 0, 10, 1, 2, 3, 4, 5]);
        let result = AgentCodec::read(&mut data).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_valid_request_identities() {
        // Valid SSH_AGENTC_REQUEST_IDENTITIES (message type 11)
        let mut data = Cursor::new(vec![0u8, 0, 0, 1, 11]);
        let result = AgentCodec::read(&mut data).await.unwrap();
        assert!(result.is_some());
    }
}
