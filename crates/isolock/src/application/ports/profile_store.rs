//! Reading and writing an immutable profile plan store.

use super::UseCaseResult;
use isolock_cas::Sha256Digest;
use isolock_plan::profile::{ProfileManifest, StorageIdentity};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The reviewed plan graph a use case works with, keyed by profile name.
pub type ProfileGraph = BTreeMap<String, ProfileManifest>;

/// Durable storage for locked plans, manifest objects, and CAS blobs.
///
/// Every method speaks in reviewed model terms; path layout, atomic writes, and locking
/// belong to the adapter that implements it.
pub trait PlanStore {
    /// Repository-wide exclusive guard held for a mutating use case.
    type Guard: LockGuard;

    /// The project directory every relative input resolves against.
    fn root(&self) -> &Path;

    /// The absolute path of the locked alias for one profile and platform.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when either identifier is not a valid name.
    fn alias(&self, profile: &str, platform: &str) -> UseCaseResult<PathBuf>;

    /// Loads the locked plan named by the alias.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when no plan is locked, or the alias does not
    /// agree with the manifest it names.
    fn load(&self, profile: &str, platform: &str) -> UseCaseResult<ProfileManifest>;

    /// Loads one immutable manifest object by content identity.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the object is missing or does not hash
    /// to the requested identity.
    fn load_object(&self, digest: &Sha256Digest) -> UseCaseResult<ProfileManifest>;

    /// Expands a manifest into its full constituent graph.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when a constituent is missing, mismatched, or
    /// forms a cycle.
    fn graph(&self, manifest: ProfileManifest) -> UseCaseResult<ProfileGraph>;

    /// Publishes the graph, optionally adding locked inputs to CAS.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when a manifest is invalid or an object
    /// cannot be written.
    fn save(&self, manifests: &ProfileGraph, capture_inputs: bool) -> UseCaseResult<()>;

    /// Lists reachable digests absent from CAS, as lowercase hex strings.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when CAS cannot be inspected.
    fn missing(&self, manifests: &ProfileGraph) -> UseCaseResult<Vec<String>>;

    /// Reports whether one digest is present and byte-verified in CAS.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when CAS cannot be inspected.
    fn verified(&self, digest: &Sha256Digest) -> UseCaseResult<bool>;

    /// Copies one already-verified local file into CAS.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the bytes cannot be stored.
    fn put_file(&self, digest: &Sha256Digest, path: &Path) -> UseCaseResult<()>;

    /// Streams bytes into CAS under `digest`.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the bytes cannot be stored.
    fn put_reader(
        &self,
        digest: &Sha256Digest,
        reader: &mut dyn std::io::Read,
    ) -> UseCaseResult<()>;

    /// The absolute path CAS uses for one digest.
    fn blob_path(&self, digest: &Sha256Digest) -> PathBuf;

    /// Writes one immutable manifest object into the plan store.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the object cannot be written.
    fn save_object(&self, manifest: &ProfileManifest) -> UseCaseResult<()>;

    /// Takes the repository-wide mutation guard.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when another operation holds it.
    fn lease(&self) -> UseCaseResult<Self::Guard>;

    /// Computes the configured source's storage identity for planning.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the source cannot be reached, or a
    /// locked identity disagrees with configuration.
    fn storage_identity(&self, locked: Option<&StorageIdentity>) -> UseCaseResult<StorageIdentity>;
}

/// An exclusive, self-releasing lock over one CAS.
pub trait LockGuard {}
