//! Adapts the directory/Git transport to the [`ArtifactTransport`] port.

use crate::application::ports::{
    ArtifactTransport, BlobStore, ProcessRunner, ProfileGraph, SourceSnapshot, UseCaseResult,
};
use crate::domain::{DomainError, Settings};
use crate::infrastructure::process::SystemProcesses;
use crate::infrastructure::transport::{self, Snapshot};
use isolock_cas::Sha256Digest;
use isolock_plan::profile::StorageIdentity;
use std::path::{Path, PathBuf};

/// A resolved source snapshot: either a directory or a pinned Git commit.
#[derive(Debug)]
pub struct FetchedSnapshot(Snapshot);

impl SourceSnapshot for FetchedSnapshot {}

/// Retrieves reviewed manifests and payloads through the configured transport.
pub struct FileTransport {
    processes: Box<dyn ProcessRunner>,
    root: PathBuf,
    settings: Settings,
}

impl FileTransport {
    /// Binds the transport to one project root and its reviewed settings.
    #[must_use]
    pub fn new(root: &Path, settings: &Settings) -> Self {
        Self {
            processes: Box::new(SystemProcesses::new()),
            root: root.into(),
            settings: settings.clone(),
        }
    }
}

impl ArtifactTransport for FileTransport {
    type Snapshot = FetchedSnapshot;

    fn snapshot(&self, locked: Option<&StorageIdentity>) -> UseCaseResult<FetchedSnapshot> {
        transport::snapshot(
            self.processes.as_ref(),
            &self.root,
            &self.settings.storage,
            locked,
        )
        .map(FetchedSnapshot)
        .map_err(DomainError::new)
    }

    fn fetch_graph(
        &self,
        store: &mut dyn BlobStore,
        source: &FetchedSnapshot,
        digest: &Sha256Digest,
    ) -> UseCaseResult<ProfileGraph> {
        transport::graph_for(self.processes.as_ref(), store, &source.0, digest)
    }

    fn fetch_blobs(
        &self,
        store: &mut dyn BlobStore,
        source: &FetchedSnapshot,
        manifests: &ProfileGraph,
    ) -> UseCaseResult<usize> {
        transport::fetch_blobs(self.processes.as_ref(), store, &source.0, manifests)
            .map_err(DomainError::new)
    }

    fn export(
        &self,
        store: &mut dyn BlobStore,
        manifests: &ProfileGraph,
        output: &Path,
    ) -> UseCaseResult<()> {
        transport::export(store, manifests, output).map_err(DomainError::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_source_is_reported_before_any_snapshot_is_taken() {
        let temp = tempfile::tempdir().expect("temp dir");
        let transport = FileTransport::new(temp.path(), &Settings::default());
        let error = transport.snapshot(None).expect_err("no source configured");
        assert!(error.to_string().contains("no artifact source configured"));
    }

    #[test]
    fn a_directory_source_resolves_against_the_project_root() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut settings = Settings::default();
        settings.storage.source = Some("storage".into());
        let transport = FileTransport::new(temp.path(), &settings);
        let snapshot = transport.snapshot(None).expect("directory source");
        let Snapshot::Directory(directory) = snapshot.0 else {
            panic!("expected a directory snapshot");
        };
        assert_eq!(directory, temp.path().join("storage"));
    }

    #[test]
    fn a_git_source_without_a_configured_source_is_rejected() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut settings = Settings::default();
        settings.storage.transport = crate::domain::Transport::Git;
        let transport = FileTransport::new(temp.path(), &settings);
        assert!(transport.snapshot(None).is_err());
    }
}
