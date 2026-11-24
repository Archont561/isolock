//! The reviewed workspace store: locked plans, immutable objects, and one CAS lease.

use crate::application::ports::{
    CollectionReport, GarbageCollector, LockGuard, PlanStore, ProfileGraph, UseCaseResult,
};
use crate::domain::{DomainError, Settings};
use crate::infrastructure::profile_store::{self, StoreLease, Workspace};
use crate::infrastructure::transport;
use isolock_cas::Sha256Digest;
use isolock_plan::profile::{ProfileManifest, StorageIdentity};
use std::path::{Path, PathBuf};

/// The repository-wide mutation guard, released on drop.
pub struct Lease(#[allow(dead_code)] StoreLease);

impl LockGuard for Lease {}

/// A workspace rooted at one project directory with one reviewed configuration.
///
/// It adapts the existing `profile_store` primitives to the [`PlanStore`] port without
/// reimplementing path layout, atomic writes, or reachability.
pub struct FilePlanStore {
    processes: Box<dyn crate::application::ports::ProcessRunner>,
    workspace: Workspace,
    settings: Settings,
}

impl FilePlanStore {
    /// Opens the workspace described by `settings` under `root`.
    #[cfg(test)]
    #[must_use]
    pub fn cas(&self) -> &isolock_cas::FileStore {
        &self.workspace.cas
    }

    #[must_use]
    pub fn new(root: &Path, settings: &Settings) -> Self {
        Self {
            processes: Box::new(crate::infrastructure::process::SystemProcesses::new()),
            workspace: Workspace::new(root, settings),
            settings: settings.clone(),
        }
    }

    /// Runs the conservative collector against explicit or discovered roots.
    ///
    /// # Errors
    /// Returns a [`DomainError`] when no roots exist or the shared CAS registration is unsafe.
    pub fn garbage_collect(
        &self,
        roots: &[String],
        apply: bool,
    ) -> UseCaseResult<profile_store::GcReport> {
        let roots = if roots.is_empty() {
            &self.settings.gc.roots
        } else {
            roots
        };
        profile_store::garbage_collect(&self.workspace, roots, apply).map_err(DomainError::new)
    }
}

impl GarbageCollector for FilePlanStore {
    fn collect(&self, roots: &[String], apply: bool) -> UseCaseResult<CollectionReport> {
        let report = self.garbage_collect(roots, apply)?;
        Ok(CollectionReport {
            roots: report.roots,
            retained: report.retained,
            reclaimable: report.reclaimable,
            bytes: report.bytes,
            applied: report.applied,
        })
    }
}

impl PlanStore for FilePlanStore {
    type Guard = Lease;

    fn root(&self) -> &Path {
        &self.workspace.root
    }

    fn alias(&self, profile: &str, platform: &str) -> UseCaseResult<PathBuf> {
        self.workspace
            .alias(profile, platform)
            .map_err(DomainError::new)
    }

    fn load(&self, profile: &str, platform: &str) -> UseCaseResult<ProfileManifest> {
        self.workspace
            .load(profile, platform)
            .map_err(DomainError::new)
    }

    fn load_object(&self, digest: &Sha256Digest) -> UseCaseResult<ProfileManifest> {
        self.workspace.load_object(digest).map_err(DomainError::new)
    }

    fn graph(&self, manifest: ProfileManifest) -> UseCaseResult<ProfileGraph> {
        self.workspace.graph(manifest).map_err(DomainError::new)
    }

    fn save(&self, manifests: &ProfileGraph, capture_inputs: bool) -> UseCaseResult<()> {
        self.workspace
            .save(manifests, capture_inputs)
            .map_err(DomainError::new)
    }

    fn missing(&self, manifests: &ProfileGraph) -> UseCaseResult<Vec<String>> {
        self.workspace.missing(manifests).map_err(DomainError::new)
    }

    fn verified(&self, digest: &Sha256Digest) -> UseCaseResult<bool> {
        self.workspace
            .cas
            .verified(digest)
            .map_err(|error| DomainError::new(error.to_string()))
    }

    fn put_file(&self, digest: &Sha256Digest, path: &Path) -> UseCaseResult<()> {
        self.workspace
            .cas
            .put_file(digest, path)
            .map(|_| ())
            .map_err(|error| DomainError::new(error.to_string()))
    }

    fn put_reader(
        &self,
        digest: &Sha256Digest,
        reader: &mut dyn std::io::Read,
    ) -> UseCaseResult<()> {
        self.workspace
            .cas
            .put_reader(digest, &mut &mut *reader)
            .map(|_| ())
            .map_err(|error| DomainError::new(error.to_string()))
    }

    fn blob_path(&self, digest: &Sha256Digest) -> PathBuf {
        self.workspace.cas.path(digest)
    }

    fn save_object(&self, manifest: &ProfileManifest) -> UseCaseResult<()> {
        let digest = manifest.digest().map_err(DomainError::new)?;
        profile_store::write_json(&self.workspace.object(&digest), manifest, true)
            .map_err(DomainError::new)
    }

    fn lease(&self) -> UseCaseResult<Lease> {
        StoreLease::acquire(&self.workspace)
            .map(Lease)
            .map_err(DomainError::new)
    }

    fn storage_identity(&self, locked: Option<&StorageIdentity>) -> UseCaseResult<StorageIdentity> {
        transport::identity(
            self.processes.as_ref(),
            &self.workspace.root,
            &self.settings.storage,
            locked,
        )
        .map_err(DomainError::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(root: &Path) -> FilePlanStore {
        FilePlanStore::new(root, &Settings::default())
    }

    #[test]
    fn a_relative_cas_directory_resolves_against_the_project_root() {
        let temp = tempfile::tempdir().expect("temp dir");
        let store = store(temp.path());
        assert_eq!(store.root(), temp.path());
        assert_eq!(
            store.cas().root(),
            temp.path().join(".isolock/cas").as_path()
        );
    }

    #[test]
    fn an_unlocked_profile_reports_the_reviewed_recovery_command() {
        let temp = tempfile::tempdir().expect("temp dir");
        let error = store(temp.path())
            .load("dev", "linux-64")
            .expect_err("no plan is locked");
        assert!(error.to_string().contains("run isolock plan --profile"));
    }

    #[test]
    fn invalid_identifiers_are_rejected_before_touching_the_store() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert!(store(temp.path()).alias("../escape", "linux-64").is_err());
    }

    #[test]
    fn the_lease_is_exclusive_and_released_on_drop() {
        let temp = tempfile::tempdir().expect("temp dir");
        let store = store(temp.path());
        let lease = store.lease().expect("first lease wins");
        assert!(store.lease().is_err());
        drop(lease);
        assert!(store.lease().is_ok());
    }
}
