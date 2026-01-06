//! Exit code definitions for authsock-filter
//!
//! Provides standardized exit codes for different error conditions.

/// Exit codes for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// Successful execution
    Success = 0,
    /// General/unspecified error
    GeneralError = 1,
    /// Configuration error (invalid config, missing required settings)
    ConfigError = 2,
    /// Socket error (cannot create/bind socket, permission denied)
    SocketError = 3,
    /// Upstream error (cannot connect to upstream agent)
    UpstreamError = 4,
}

impl From<ExitCode> for u8 {
    fn from(code: ExitCode) -> Self {
        code as u8
    }
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(code: ExitCode) -> Self {
        std::process::ExitCode::from(code as u8)
    }
}
