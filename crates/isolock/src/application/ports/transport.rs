//! Moving reviewed bytes between workspaces without letting use cases run Git or read CAS.

use super::profile_store::ProfileGraph;
use super::UseCaseResult;
use isolock_cas::Sha256Digest;
use isolock_plan::profile::{ProfileManifest, StorageIdentity};
use std::io::Read;
use std::path::Path;

/// A snapshot of the configured artifact source, as the transport adapter sees it.
pub trait SourceSnapshot {}

/// Retrieves reviewed manifests and payloads from a configured source.
pub trait ArtifactTransport {
    /// The source snapshot handle this transport resolves for a locked identity.
    type Snapshot: SourceSnapshot;

    /// Resolves the configured source, optionally pinned to a locked identity.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when no source is configured, or the snapshot
    /// does not match the locked commit.
    fn snapshot(&self, locked: Option<&StorageIdentity>) -> UseCaseResult<Self::Snapshot>;

    /// Fetches the manifest graph into the store before any payload byte is copied.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when a manifest is missing, mismatched, or the
    /// graph is inconsistent.
    fn fetch_graph(
        &self,
        store: &mut dyn BlobStore,
        source: &Self::Snapshot,
        digest: &Sha256Digest,
    ) -> UseCaseResult<ProfileGraph>;

    /// Copies every reachable payload into CAS, returning how many were transferred.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when a payload is missing from the source or
    /// cannot be stored.
    fn fetch_blobs(
        &self,
        store: &mut dyn BlobStore,
        source: &Self::Snapshot,
        manifests: &ProfileGraph,
    ) -> UseCaseResult<usize>;

    /// Writes the selected graph as an offline directory snapshot.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when a blob is absent or a file cannot be
    /// written.
    fn export(
        &self,
        store: &mut dyn BlobStore,
        manifests: &ProfileGraph,
        output: &Path,
    ) -> UseCaseResult<()>;
}

/// The reviewed store capabilities transport and materialization need.
///
/// A fake satisfies it without a filesystem, which is what makes retrieval use cases testable.
pub trait BlobStore {
    /// Writes one verified manifest object into the store.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the object cannot be written.
    fn write_manifest(&mut self, manifest: &ProfileManifest) -> UseCaseResult<()>;

    /// Reports whether CAS already holds a verified copy.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when CAS cannot be inspected.
    fn holds(&self, digest: &Sha256Digest) -> UseCaseResult<bool>;

    /// Reads one blob's absolute path from the project CAS.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the blob is absent.
    fn path_of(&self, digest: &Sha256Digest) -> UseCaseResult<std::path::PathBuf>;

    /// Stores a file already verified under `digest`.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the bytes cannot be stored.
    fn put_file(&mut self, digest: &Sha256Digest, path: &Path) -> UseCaseResult<()>;

    /// Streams bytes into CAS under `digest`.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the bytes cannot be stored.
    fn put_reader(&mut self, digest: &Sha256Digest, reader: &mut dyn Read) -> UseCaseResult<()>;
}
