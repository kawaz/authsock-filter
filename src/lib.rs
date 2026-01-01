//! authsock-filter - SSH agent proxy with filtering and logging
//!
//! This library provides functionality to create filtered SSH agent sockets
//! that proxy requests to an upstream SSH agent while applying configurable
//! filters to control which keys are visible and which operations are allowed.

pub mod agent;
pub mod cli;
pub mod config;
pub mod error;
pub mod filter;
pub mod logging;
pub mod protocol;
pub mod service;

pub use error::{Error, Result};

/// Package version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Package name
pub const NAME: &str = env!("CARGO_PKG_NAME");
