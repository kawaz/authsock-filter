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

