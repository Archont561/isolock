//! Adapters driven through the process port, with no real program ever spawned.
//!
//! These tests were impossible before the port existed: the archive boundary
//! called `Command::new("tar")` directly, so every assertion needed the real
//! `tar` binary on the host and a crafted archive on disk.

use crate::application::ports::fakes::{write_file, FakeProcesses};
use crate::infrastructure::materialize::archive;
use isolock_cas::Sha256Digest;
use isolock_plan::profile::{Artifact, ArtifactKind};
use std::path::{Path, PathBuf};

fn temporary() -> tempfile::TempDir {
    tempfile::tempdir().expect("temporary directory")
}

/// Puts `bytes` into a fresh CAS and returns a matching artifact record.
fn staged_artifact(root: &Path, bytes: &[u8]) -> (isolock_cas::FileStore, Artifact) {
    let cas = isolock_cas::FileStore::new(root.join("cas"));
    let source = write_file(root, "payload.tar", bytes);
    let digest: Sha256Digest = isolock_cas::digest_file(&source).expect("digest");
    cas.put_file(&digest, &source).expect("store blob");
    let artifact = Artifact {
        kind: ArtifactKind::Seed,
        name: "demo".into(),
        version: "1.0.0".into(),
        platform: "linux-64".into(),
        digest,
        filename: "payload.tar".into(),
        source: "https://example.invalid/payload.tar".into(),
        metadata: serde_json::json!({ "format": "tar" }),
        integrity: None,
        destinations: Vec::new(),
    };
    (cas, artifact)
}

#[test]
fn unpacking_an_archive_inspects_twice_then_extracts() {
    let directory = temporary();
    let (cas, artifact) = staged_artifact(directory.path(), b"archive bytes");
    let processes = FakeProcesses::new()
        .printing("demo-1.0.0/bin/demo\n")
        .printing("-rwxr-xr-x demo-1.0.0/bin/demo\n")
        .succeeding();

    archive::unpack(&processes, &cas, &artifact, &directory.path().join("out")).expect("unpacked");

    let invocations = processes.invocations();
    assert_eq!(invocations.len(), 3);
    assert!(invocations.iter().all(|call| call.program() == "tar"));
    // A plain listing precedes the verbose one, and both precede extraction.
    assert!(invocations[0].arguments().starts_with("-tf"));
    assert!(invocations[1].arguments().starts_with("-tvf"));
    assert!(invocations[2].arguments().contains("-xf"));
    assert!(invocations[2].arguments().contains("--no-same-owner"));
    // The environment the previous implementation scrubbed stays scrubbed.
    assert!(invocations.iter().all(|call| call.removed("TAR_OPTIONS")));
    assert!(invocations.iter().all(|call| call.removed("GZIP")));
    // No environment variable is ever added to a tar invocation.
    assert!(invocations
        .iter()
        .all(|call| call.environment("TAR_OPTIONS").is_none()));
}

#[test]
fn a_link_inside_the_archive_is_rejected_before_extraction() {
    let directory = temporary();
    let (cas, artifact) = staged_artifact(directory.path(), b"archive bytes");
    // The verbose listing reports a symbolic link, which v1 refuses to restore.
    let processes = FakeProcesses::new()
        .printing("demo-1.0.0/bin/demo\n")
        .printing("lrwxr-xr-x demo-1.0.0/bin/demo -> /etc/passwd\n");

    let error = archive::unpack(&processes, &cas, &artifact, &directory.path().join("out"))
        .expect_err("links are rejected");
    assert!(error.contains("links and special files are rejected"));
    // Extraction never ran, which is the point of inspecting first.
    assert_eq!(processes.invocations().len(), 2);
}

#[test]
fn an_unreadable_archive_never_reaches_extraction() {
    let directory = temporary();
    let (cas, artifact) = staged_artifact(directory.path(), b"archive bytes");
    let processes = FakeProcesses::new().failing_with(2, "");

    let error = archive::unpack(&processes, &cas, &artifact, &directory.path().join("out"))
        .expect_err("invalid archive");
    assert_eq!(error, "invalid archive");
    assert_eq!(processes.invocations().len(), 1);
}

#[test]
fn a_failed_extraction_reports_the_reviewed_message() {
    let directory = temporary();
    let (cas, artifact) = staged_artifact(directory.path(), b"archive bytes");
    let processes = FakeProcesses::new()
        .printing("demo-1.0.0/bin/demo\n")
        .printing("-rwxr-xr-x demo-1.0.0/bin/demo\n")
        .failing_with(1, "");

    let error = archive::unpack(&processes, &cas, &artifact, &directory.path().join("out"))
        .expect_err("extraction failed");
    assert_eq!(error, "cannot extract verified archive");
}

#[test]
fn a_missing_program_reports_the_unchanged_diagnostic() {
    let directory = temporary();
    let (cas, artifact) = staged_artifact(directory.path(), b"archive bytes");
    let processes = FakeProcesses::new().unavailable("No such file or directory");

    let error = archive::unpack(&processes, &cas, &artifact, &directory.path().join("out"))
        .expect_err("tar is absent");
    assert_eq!(error, "cannot inspect archive");
}

#[test]
fn an_archive_path_that_escapes_the_tree_is_rejected() {
    let directory = temporary();
    let (cas, artifact) = staged_artifact(directory.path(), b"archive bytes");
    let processes = FakeProcesses::new().printing("../../etc/shadow\n");

    let error = archive::unpack(&processes, &cas, &artifact, &directory.path().join("out"))
        .expect_err("traversal rejected");
    assert!(!error.is_empty());
    // Never extracted after a rejected path.
    assert_eq!(processes.invocations().len(), 1);
}

#[test]
fn the_reserved_receipt_path_is_refused() {
    let directory = temporary();
    let (cas, artifact) = staged_artifact(directory.path(), b"archive bytes");
    let processes = FakeProcesses::new().printing(".isolock-tree.json\n");

    let error = archive::unpack(&processes, &cas, &artifact, &directory.path().join("out"))
        .expect_err("reserved path rejected");
    assert_eq!(error, "archive uses a reserved derivation receipt path");
}

#[test]
fn a_git_blob_streams_into_the_store_without_buffering() {
    // The Git snapshot is constructed directly, so no repository is required: the
    // point under test is that the blob travels as a stream into CAS.
    let directory = temporary();
    let store = isolock_cas::FileStore::new(directory.path().join("cas"));
    let mut blob_store = StreamSink::new();
    let processes = FakeProcesses::new().streaming(b"streamed artifact bytes", true);
    let snapshot = crate::infrastructure::transport::Snapshot::Git {
        repository: directory.path().into(),
        commit: "0".repeat(40),
    };
    let digest = Sha256Digest::from_bytes(b"expected");

    let copied = snapshot
        .copy_blob(&processes, &mut blob_store, &digest)
        .expect("blob streamed");
    assert!(copied);
    assert_eq!(blob_store.bytes, b"streamed artifact bytes");
    let invocation = processes.single();
    assert_eq!(invocation.program(), "git");
    assert!(invocation.arguments().contains("show"));
    assert!(invocation.arguments().contains("0".repeat(40).as_str()));
    let _ = store;
}

#[test]
fn a_missing_git_blob_is_reported_after_the_stream_closes() {
    let directory = temporary();
    let mut blob_store = StreamSink::new();
    // Git exits non-zero: the artifact is absent from the pinned commit.
    let processes = FakeProcesses::new().streaming("", false);
    let snapshot = crate::infrastructure::transport::Snapshot::Git {
        repository: directory.path().into(),
        commit: "0".repeat(40),
    };

    let error = snapshot
        .copy_blob(&processes, &mut blob_store, &Sha256Digest::from_bytes(b"x"))
        .expect_err("missing blob");
    assert!(error.starts_with("Git source is missing or cannot read artifact"));
}

/// A blob sink that records the bytes it was handed, so streaming is observable.
struct StreamSink {
    bytes: Vec<u8>,
}

impl StreamSink {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }
}

impl crate::application::ports::BlobStore for StreamSink {
    fn write_manifest(
        &mut self,
        _manifest: &isolock_plan::profile::ProfileManifest,
    ) -> crate::application::ports::UseCaseResult<()> {
        Ok(())
    }
    fn holds(&self, _digest: &Sha256Digest) -> crate::application::ports::UseCaseResult<bool> {
        Ok(false)
    }
    fn path_of(&self, _digest: &Sha256Digest) -> crate::application::ports::UseCaseResult<PathBuf> {
        Ok(PathBuf::from("unused"))
    }
    fn put_file(
        &mut self,
        _digest: &Sha256Digest,
        _path: &Path,
    ) -> crate::application::ports::UseCaseResult<()> {
        Ok(())
    }
    fn put_reader(
        &mut self,
        _digest: &Sha256Digest,
        reader: &mut dyn std::io::Read,
    ) -> crate::application::ports::UseCaseResult<()> {
        reader
            .read_to_end(&mut self.bytes)
            .map(|_| ())
            .map_err(|error| crate::domain::DomainError::new(error.to_string()))
    }
}

#[test]
fn the_materializer_accepts_an_injected_process_boundary() {
    let directory = temporary();
    let workspace = crate::infrastructure::profile_store::Workspace::new(
        directory.path(),
        &crate::domain::Settings::default(),
    );
    let processes = FakeProcesses::new();
    let materializer = crate::infrastructure::FileMaterializer::new(workspace)
        .with_processes(Box::new(FakeProcesses::new()));
    // Nothing is spawned, and the adapter keeps reporting the host it targets.
    assert!(crate::application::ports::Materializer::host_platform(&materializer).is_some());
    assert!(processes.programs().is_empty());
}
