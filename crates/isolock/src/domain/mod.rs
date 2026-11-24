//! Pure policy shared by every layer; nothing here reads files, processes, or argv.
//!
//! The crate-level domain is deliberately small. Executable model types already live in
//! `isolock-core`, `isolock-plan`, `isolock-cas`, and `isolock-proxy`; this module holds
//! only the value types that describe a *reviewed workspace* and the invariants over them.

pub mod configuration;
pub mod error;

pub use configuration::{ProxyMode, ProxySettings, Settings, Storage, Transport};
pub use error::DomainError;
