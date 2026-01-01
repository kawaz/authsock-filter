//! SSH Agent protocol codec for tokio

use crate::error::{Error, Result};
use crate::protocol::message::AgentMessage;
use bytes::{Buf, BufMut, BytesMut};
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

/// Buffer-based codec for use with split streams
pub struct AgentCodecBuffer {
    buffer: BytesMut,
}

impl AgentCodecBuffer {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
        }
    }

    /// Try to decode a message from the buffer
    pub fn decode(&mut self) -> Result<Option<AgentMessage>> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }

        let len = u32::from_be_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
        ]) as usize;

        if len == 0 {
            return Err(Error::InvalidMessage("Zero-length message".to_string()));
        }
        if len > MAX_MESSAGE_SIZE as usize {
            return Err(Error::InvalidMessage(format!(
                "Message too large: {} bytes",
                len
            )));
        }

        if self.buffer.len() < 4 + len {
            return Ok(None);
        }

        self.buffer.advance(4);
        let data = self.buffer.split_to(len);
        let msg = AgentMessage::decode(&data)?;
        Ok(Some(msg))
    }

    /// Encode a message to bytes
    pub fn encode(&self, msg: &AgentMessage) -> BytesMut {
        let total_len = 1 + msg.payload.len();
        let mut buf = BytesMut::with_capacity(4 + total_len);
        buf.put_u32(total_len as u32);
        buf.put_u8(msg.msg_type.into());
        buf.put_slice(&msg.payload);
        buf
    }

    /// Add data to the internal buffer
    pub fn extend(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }
}

impl Default for AgentCodecBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::MessageType;

    #[test]
    fn test_codec_buffer_decode() {
        let mut codec = AgentCodecBuffer::new();

        // Empty buffer should return None
        assert!(codec.decode().unwrap().is_none());

        // Add a complete message (length=1, type=11)
        codec.extend(&[0, 0, 0, 1, 11]);

        let msg = codec.decode().unwrap().unwrap();
        assert_eq!(msg.msg_type, MessageType::RequestIdentities);
        assert!(msg.payload.is_empty());
    }

    #[test]
    fn test_codec_buffer_partial() {
        let mut codec = AgentCodecBuffer::new();

        // Add partial length
        codec.extend(&[0, 0]);
        assert!(codec.decode().unwrap().is_none());

        // Complete length but no body
        codec.extend(&[0, 1]);
        assert!(codec.decode().unwrap().is_none());

        // Complete message
        codec.extend(&[11]);
        let msg = codec.decode().unwrap().unwrap();
        assert_eq!(msg.msg_type, MessageType::RequestIdentities);
    }
}
