//! SSH Agent proxy module
//!
//! This module provides components for creating a filtered SSH agent proxy:
//! - `Upstream`: Connection to the upstream SSH agent
//! - `Server`: Unix socket server for accepting client connections
//! - `Proxy`: Core proxy logic that filters requests between client and upstream

mod proxy;
mod server;
mod upstream;

pub use proxy::Proxy;
pub use server::Server;
pub use upstream::Upstream;
