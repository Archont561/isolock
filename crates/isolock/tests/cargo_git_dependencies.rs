//! A Cargo git dependency is ingested as an exported commit tree, not a URL.
//!
//! Real `git` and a real CAS layout are the seam: the point of the feature is
//! that a reviewer pins a commit and the bytes are reproducible, so a fake
//! would prove nothing.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Fixtures fail fast on invalid setup.

use isolock_cas::{digest_file, FileStore, Sha256Digest};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

struct World {
    temp: TempDir,
    commit: String,
}

fn git(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "reviewed")
        .env("GIT_AUTHOR_EMAIL", "reviewed@example.test")
        .env("GIT_COMMITTER_NAME", "reviewed")
        .env("GIT_COMMITTER_EMAIL", "reviewed@example.test")
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

impl World {
    /// A real repository holding one crate, with a fixed author timestamp so the
    /// commit SHA is stable across runs.
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let upstream = temp.path().join("upstream");
        fs::create_dir_all(upstream.join("src")).unwrap();
        fs::write(
            upstream.join("Cargo.toml"),
            "[package]\nname = \"gitdep\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(
            upstream.join("src/lib.rs"),
            "pub fn answer() -> u32 { 42 }\n",
        )
        .unwrap();
        git(&upstream, &["init", "--quiet", "."]);
        git(&upstream, &["add", "-A"]);
        git(&upstream, &["commit", "--quiet", "-m", "one"]);
        let commit = git(&upstream, &["rev-parse", "HEAD"]);
        Self { temp, commit }
    }

    fn upstream(&self) -> PathBuf {
        self.temp.path().join("upstream")
    }

    /// The digest `git archive` produces for the pinned commit, computed
    /// independently of the code under test.
    fn expected(&self) -> Sha256Digest {
        let archive = self.temp.path().join("expected.tar");
        let output = Command::new("git")
            .current_dir(self.upstream())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "archive",
                "--format=tar",
                "--output",
                archive.to_str().unwrap(),
                &self.commit,
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        digest_file(&archive).unwrap()
    }

    fn manifest(&self, digest: &Sha256Digest, commit: &str) -> PathBuf {
        let path = self.temp.path().join("isolock.ingest.json");
        let source = format!("file://{}", self.upstream().to_str().unwrap());
        fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({
                "schema-version": 1,
                "artifacts": [{
                    "registry": "cargo",
                    "name": "gitdep",
                    "version": "0.1.0",
                    "sha256": digest,
                    "filename": "gitdep-git.tar",
                    "source": source,
                    "git": {
                        "repository": source,
                        "revision": commit,
                        "commit": commit
                    }
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        path
    }

    fn settings(&self) {
        fs::write(
            self.temp.path().join("isolock.toml"),
            "[storage]\ncas-dir = 'custom-cas'\n",
        )
        .unwrap();
    }

    fn run(&self, arguments: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", self.temp.path().join("empty-home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .current_dir(self.temp.path())
            .args(arguments);
        test_coverage::preserve_profile(&mut command);
        command.output().unwrap()
    }

    fn ingest(&self) -> Output {
        self.run(&[
            "ingest",
            "--manifest",
            "isolock.ingest.json",
            "--artifacts",
            "originals",
        ])
    }
}

/// Keeps `coverage` environment variables that the profile needs to attribute runs.
#[allow(dead_code)]
mod test_coverage {
    pub fn preserve_profile(command: &mut std::process::Command) {
        if let Some(path) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", path);
        }
    }
}

#[test]
fn a_pinned_git_commit_is_ingested_as_reproducible_archive_bytes() {
    let world = World::new();
    let digest = world.expected();
    let manifest = world.manifest(&digest, &world.commit);
    assert!(manifest.exists());
    fs::create_dir(world.temp.path().join("originals")).unwrap();
    world.settings();

    let output = world.ingest();
    assert!(output.status.success(), "{output:?}");

    // The exported tree, not a download, is what CAS holds.
    let store = FileStore::new(world.temp.path().join("custom-cas"));
    assert!(store.verified(&digest).unwrap());
    let bytes = fs::read(store.path(&digest)).unwrap();
    assert!(bytes.windows(9).any(|window| window == b"src/lib.r"));
}

#[test]
fn the_same_commit_yields_the_same_digest_on_a_second_export() {
    let world = World::new();
    let first = world.expected();
    let second = world.expected();
    assert_eq!(first, second, "git archive must be reproducible");
}

#[test]
fn a_reviewed_revision_that_no_longer_resolves_to_its_commit_is_rejected() {
    let world = World::new();
    let digest = world.expected();
    // The reviewer pinned a well-formed commit that the repository does not have.
    let unknown = "0".repeat(40);
    world.manifest(&digest, &unknown);
    fs::create_dir(world.temp.path().join("originals")).unwrap();
    world.settings();

    let output = world.ingest();
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Cargo git dependency"),
        "expected a reviewed-input failure, got {stderr}"
    );
    let store = FileStore::new(world.temp.path().join("custom-cas"));
    assert!(!store.verified(&digest).unwrap());
}

#[test]
fn a_git_pin_with_a_wrong_digest_is_rejected_and_cas_is_untouched() {
    let world = World::new();
    let digest = Sha256Digest::from_bytes(b"not the exported tree");
    world.manifest(&digest, &world.commit);
    fs::create_dir(world.temp.path().join("originals")).unwrap();
    world.settings();

    let output = world.ingest();
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not match its pinned digest"),
        "expected a digest mismatch, got {stderr}"
    );
    let store = FileStore::new(world.temp.path().join("custom-cas"));
    assert!(!store.verified(&digest).unwrap());
}

#[test]
fn a_git_pin_cannot_be_downloaded_from_its_url() {
    let world = World::new();
    let digest = world.expected();
    world.manifest(&digest, &world.commit);
    world.settings();

    let output = world.run(&["ingest", "--manifest", "isolock.ingest.json", "--download"]);
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("acquired by revision, not by URL"),
        "expected a revision-only failure, got {stderr}"
    );
}

#[test]
fn a_git_pin_on_a_non_cargo_artifact_is_rejected() {
    let world = World::new();
    let digest = world.expected();
    let path = world.manifest(&digest, &world.commit);
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    document["artifacts"][0]["registry"] = json!("pypi");
    fs::write(&path, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
    world.settings();

    let output = world.ingest();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("only supported for Cargo"));
}
