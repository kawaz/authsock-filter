//! Service management module
//!
//! This module provides functionality for managing authsock-filter as a background service:
//! - Daemon control (start/stop/status)
//! - macOS launchd integration
//! - Linux systemd integration

mod daemon;
mod launchd;
mod systemd;

pub use daemon::{Daemon, DaemonStatus};
pub use launchd::{Launchd, LaunchdStatus};
pub use systemd::{Systemd, SystemdStatus};
