//! Command implementations for authsock-filter CLI

pub mod completion;
pub mod config;
pub mod run;
pub mod service;
pub mod version;

pub use crate::utils::version_manager::{
    VersionManagerInfo, detect_version_manager, find_shim_suggestions, is_executable,
};
