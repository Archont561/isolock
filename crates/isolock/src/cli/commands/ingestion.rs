//! Reviewed ingestion and explicit publication retain the declarative controller grammar.

use std::path::PathBuf;

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Ingest {
    /// Reviewed JSON pins; this command never resolves package versions.
    #[clifacet(long, default_value = "isolock.ingest.json")]
    pub(crate) manifest: PathBuf,
    /// Directory containing the original archive basenames named in the manifest.
    #[clifacet(long, default_value = ".")]
    pub(crate) artifacts: PathBuf,
    /// Explicitly acquire the pinned HTTPS sources using curl instead of local files.
    #[clifacet(long, conflicts_with = "artifacts")]
    pub(crate) download: bool,
    /// Optionally restrict the reviewed batch to one ecosystem.
    #[clifacet(long, value_parser = ["cargo", "pypi", "npm", "conda", "generic"])]
    pub(crate) registry: Option<String>,
    /// Optional reviewed NAME==VERSION selections (requires --registry).
    pub(crate) packages: Vec<String>,
}

#[clifacet::group]
#[command(display_name = "isolock")]
pub struct Push {
    /// Reviewed JSON pins already verified in the configured CAS.
    #[clifacet(long, default_value = "isolock.ingest.json")]
    pub(crate) manifest: PathBuf,
    /// Git remote name, local repository directory, or credential-free HTTPS URL.
    #[clifacet(long, default_value = "origin")]
    pub(crate) remote: String,
    /// Reviewed source-pinned budgets and retention roots for automatic epoch rollover.
    #[clifacet(long)]
    pub(crate) epoch_policy: Option<PathBuf>,
    #[clifacet(long, value_parser = ["cargo", "pypi", "npm", "conda", "generic"])]
    pub(crate) registry: Option<String>,
    pub(crate) packages: Vec<String>,
}
