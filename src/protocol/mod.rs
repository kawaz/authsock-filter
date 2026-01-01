//! SSH Agent Protocol implementation
//!
//! This module implements the SSH agent protocol as defined in:
//! https://datatracker.ietf.org/doc/html/draft-miller-ssh-agent

pub mod codec;
pub mod message;

pub use codec::AgentCodec;
pub use message::{AgentMessage, Identity, MessageType};
