//! Installing a locked profile on this host is infrastructure; deciding to is policy.

use super::profile_store::ProfileGraph;
use super::UseCaseResult;
use isolock_plan::profile::ProfileManifest;
use std::path::{Path, PathBuf};

/// Writes materialized prefixes and POSIX seed files for a reviewed graph.
pub trait Materializer {
    /// Installs every constituent and returns the selected profile's prefix.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when a payload is missing, unsafe, or cannot
    /// be written.
    fn install(&self, manifests: &ProfileGraph, selected: &str) -> UseCaseResult<PathBuf>;

    /// Renders the reviewed POSIX bootstrap manifest for one selected profile.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the manifest cannot be rendered.
    fn bootstrap_manifest(&self, manifest: &ProfileManifest) -> UseCaseResult<String>;

    /// Writes the seed manifest and the reviewed bootstrap script.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when either file cannot be written.
    fn write_seed(&self, manifest_path: &Path, contents: &str) -> UseCaseResult<()>;

    /// The reviewed seed manifest path for one profile and platform.
    fn seed_path(&self, profile: &str, platform: &str) -> UseCaseResult<PathBuf>;

    /// The host platform this process can materialize, if it is supported.
    fn host_platform(&self) -> Option<String>;
}
