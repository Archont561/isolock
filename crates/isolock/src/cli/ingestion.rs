//! Reviewed ingestion and explicit publication retain the declarative controller grammar.

use std::path::PathBuf;

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Ingest {
    /// Reviewed JSON pins; this command never resolves package versions.
    #[clifacet(long, default_value = "isolock.ingest.json")]
    pub(super) manifest: PathBuf,
    /// Directory containing the original archive basenames named in the manifest.
    #[clifacet(long, default_value = ".")]
    pub(super) artifacts: PathBuf,
    /// Explicitly acquire the pinned HTTPS sources using curl instead of local files.
    #[clifacet(long, conflicts_with = "artifacts")]
    pub(super) download: bool,
    /// Optionally restrict the reviewed batch to one ecosystem.
    #[clifacet(long, value_parser = ["cargo", "pypi", "npm", "conda", "generic"])]
    pub(super) registry: Option<String>,
    /// Optional reviewed NAME==VERSION selections (requires --registry).
    pub(super) packages: Vec<String>,
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Push {
    /// Reviewed JSON pins already verified in the configured CAS.
    #[clifacet(long, default_value = "isolock.ingest.json")]
    pub(super) manifest: PathBuf,
    /// Git remote name, local repository directory, or credential-free HTTPS URL.
    #[clifacet(long, default_value = "origin")]
    pub(super) remote: String,
    #[clifacet(long, value_parser = ["cargo", "pypi", "npm", "conda", "generic"])]
    pub(super) registry: Option<String>,
    pub(super) packages: Vec<String>,
}
