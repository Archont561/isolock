//! Reviewed ingestion and publication, expressed without any clap or Figment coupling.

use super::UseCaseResult;
use std::path::Path;

/// The reviewed NAME==VERSION selection within one ecosystem.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection<'a> {
    /// One ecosystem, or every ecosystem when absent.
    pub registry: Option<&'a str>,
    /// The reviewed package selections.
    pub packages: &'a [String],
}

/// One reviewed publication request.
pub struct Publication<'a> {
    /// The exported snapshot of the currently locked profile.
    pub snapshot: &'a dyn ProfileSnapshot,
    /// Reviewed JSON pins already verified in CAS.
    pub manifest: &'a std::path::Path,
    /// Git remote name, local repository directory, or HTTPS URL.
    pub remote: &'a str,
    /// Reviewed epoch policy, when publication rolls over.
    pub epoch_policy: Option<&'a std::path::Path>,
    /// The reviewed ecosystem/package selection.
    pub selection: Selection<'a>,
}

/// Explicitly importing reviewed original bytes into CAS.
pub trait Ingestion {
    /// Verifies and imports the selected batch, returning the operator-facing summary.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the manifest is invalid, a blob does not
    /// match its pin, or the store cannot be updated.
    fn ingest(
        &self,
        manifest: &Path,
        artifacts: &Path,
        selection: Selection<'_>,
        download: bool,
    ) -> UseCaseResult<String>;

    /// Publishes already-verified reviewed artifacts to storage.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the batch is unverified, the storage tip
    /// moved, or publication fails.
    fn publish(&self, request: Publication<'_>) -> UseCaseResult<String>;
}

/// A profile publication needs an exported snapshot of the currently locked profile.
pub trait ProfileSnapshot {
    /// The selected profile and platform, reported by the publication itself.
    fn selection(&self) -> (String, String);

    /// Writes the verified offline snapshot of the selected profile to `output`.
    ///
    /// # Errors
    /// Returns a message when the locked profile cannot be exported.
    fn snapshot(&self, output: &Path) -> Result<(), String>;
}
