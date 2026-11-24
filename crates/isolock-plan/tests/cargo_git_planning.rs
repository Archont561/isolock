//! Cargo git dependencies are planned only from a reviewed pin, never derived.
//!
//! A `Cargo.lock` records a commit but no content hash for a git package, so the
//! digest has to come from a reviewer. These tests hold that boundary: the pin
//! must match the lockfile exactly, and anything unpinnable fails closed.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid contract fixtures must fail tests.

use isolock_cas::Sha256Digest;
use isolock_plan::profile::{
    build_profiles, ArchiveFormat, ArtifactKind, CargoGitPin, CargoPolicy, PlanRequest,
    PlatformTools, Profile, ProfileManifest, Provider, StorageIdentity, ToolArtifact,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const PLATFORM: &str = "linux-64";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const REPOSITORY: &str = "https://example.test/org/lib";

/// A lockfile naming one app, one git crate, and one registry crate.
fn lockfile(commit: &str) -> String {
    format!(
        r#"version = 4

[[package]]
name = "app"
version = "0.1.0"

[[package]]
name = "lib"
version = "0.1.0"
source = "git+{REPOSITORY}?rev={commit}#{commit}"

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "{checksum}"
"#,
        checksum = "a".repeat(64),
    )
}

/// A leaf profile needs a verified isolock seed; unrelated to this feature.
fn tools() -> PlatformTools {
    PlatformTools {
        seeds: BTreeMap::from([(
            "isolock".into(),
            ToolArtifact {
                version: "1.0.0".into(),
                platform: PLATFORM.into(),
                digest: Sha256Digest::from_bytes(b"seed"),
                filename: "isolock".into(),
                source: "https://example.test/isolock".into(),
                format: ArchiveFormat::Raw,
                entry: None,
            },
        )]),
        ..PlatformTools::default()
    }
}

fn pin(name: &str, version: &str, commit: &str, repository: &str) -> CargoGitPin {
    CargoGitPin {
        name: name.into(),
        version: version.into(),
        repository: repository.into(),
        commit: commit.into(),
        digest: Sha256Digest::from_bytes(b"exported commit tree"),
    }
}

struct Project {
    temp: tempfile::TempDir,
}

impl Project {
    fn new() -> Self {
        Self {
            temp: tempfile::tempdir().unwrap(),
        }
    }

    fn root(&self) -> &Path {
        self.temp.path()
    }

    /// Writes the lockfile and returns the profile that selects it.
    fn profile(&self, source: &str, pins: Vec<CargoGitPin>) -> Profile {
        fs::write(self.root().join("Cargo.lock"), source).unwrap();
        Profile {
            provider: Provider::Standalone,
            platforms: vec![PLATFORM.into()],
            cargo_lock: Some("Cargo.lock".into()),
            cargo: CargoPolicy {
                cargo_version: "1.88.0".into(),
                target: "x86_64-unknown-linux-gnu".into(),
                features: Vec::new(),
                all_features: true,
            },
            cargo_git: pins,
            platform_tools: BTreeMap::from([(PLATFORM.to_owned(), tools())]),
            ..Profile::default()
        }
    }

    fn plan(&self, profile: &Profile) -> Result<BTreeMap<String, ProfileManifest>, String> {
        let profiles = BTreeMap::from([("rust".to_owned(), profile.clone())]);
        build_profiles(&PlanRequest {
            root: self.root(),
            profile: "rust",
            platform: PLATFORM,
            profiles: &profiles,
            storage: StorageIdentity::default(),
        })
    }
}

fn cargo_artifacts(manifest: &ProfileManifest) -> Vec<&isolock_plan::profile::Artifact> {
    manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Cargo)
        .collect()
}

#[test]
fn a_reviewed_pin_plans_the_git_dependency_with_its_archive_digest() {
    let project = Project::new();
    let profile = project.profile(
        &lockfile(COMMIT),
        vec![pin("lib", "0.1.0", COMMIT, REPOSITORY)],
    );
    let plans = project.plan(&profile).unwrap();
    let artifacts = cargo_artifacts(&plans["rust"]);
    assert_eq!(artifacts.len(), 2, "registry plus git");

    let git = artifacts
        .iter()
        .find(|artifact| artifact.name == "lib")
        .unwrap();
    assert_eq!(git.filename, "lib-git.tar");
    assert_eq!(git.metadata["origin"], "git");
    assert_eq!(git.metadata["revision"], COMMIT);
    assert_eq!(git.metadata["repository"], REPOSITORY);
    assert_eq!(
        git.digest,
        Sha256Digest::from_bytes(b"exported commit tree")
    );
    // The source stays a bare repository URL: `public_source` refuses `?` and `#`,
    // so the rev travels in metadata for materialisation to build Cargo's key from.
    assert_eq!(git.source, REPOSITORY);
    assert_eq!(git.metadata["reference"], format!("rev={COMMIT}"));
}

#[test]
fn a_git_dependency_without_a_pin_still_fails_closed() {
    let project = Project::new();
    let profile = project.profile(&lockfile(COMMIT), Vec::new());
    let error = project.plan(&profile).unwrap_err();
    assert!(
        error.contains("requires exactly one reviewed pin"),
        "{error}"
    );
}

#[test]
fn a_pin_for_a_different_commit_is_rejected() {
    let project = Project::new();
    let other = "f".repeat(40);
    let profile = project.profile(
        &lockfile(COMMIT),
        vec![pin("lib", "0.1.0", &other, REPOSITORY)],
    );
    let error = project.plan(&profile).unwrap_err();
    assert!(
        error.contains("does not match the lockfile commit"),
        "{error}"
    );
}

#[test]
fn a_pin_for_a_different_repository_is_rejected() {
    let project = Project::new();
    let profile = project.profile(
        &lockfile(COMMIT),
        vec![pin(
            "lib",
            "0.1.0",
            COMMIT,
            "https://example.test/other/lib",
        )],
    );
    let error = project.plan(&profile).unwrap_err();
    assert!(
        error.contains("does not match the lockfile repository"),
        "{error}"
    );
}

#[test]
fn a_lockfile_that_pins_only_a_branch_or_tag_is_rejected() {
    let project = Project::new();
    // `?rev=main#main`: Cargo recorded a reference, not a resolved commit.
    let source = lockfile(COMMIT).replace(&format!("?rev={COMMIT}#{COMMIT}"), "?rev=main#main");
    let profile = project.profile(&source, vec![pin("lib", "0.1.0", COMMIT, REPOSITORY)]);
    let error = project.plan(&profile).unwrap_err();
    assert!(error.contains("full lowercase commit"), "{error}");
}

#[test]
fn a_pin_that_matches_no_lockfile_git_dependency_is_rejected() {
    let project = Project::new();
    let profile = project.profile(
        &lockfile(COMMIT),
        vec![
            pin("lib", "0.1.0", COMMIT, REPOSITORY),
            pin("ghost", "9.9.9", COMMIT, REPOSITORY),
        ],
    );
    let error = project.plan(&profile).unwrap_err();
    assert!(error.contains("is not a git dependency"), "{error}");
}

#[test]
fn two_pins_for_the_same_package_are_rejected() {
    let project = Project::new();
    let profile = project.profile(
        &lockfile(COMMIT),
        vec![
            pin("lib", "0.1.0", COMMIT, REPOSITORY),
            pin("lib", "0.1.0", COMMIT, REPOSITORY),
        ],
    );
    let error = project.plan(&profile).unwrap_err();
    assert!(error.contains("exactly one reviewed pin"), "{error}");
}

#[test]
fn a_registry_only_lockfile_is_unchanged_by_the_new_field() {
    let project = Project::new();
    let source = format!(
        r#"version = 4

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "{}"
"#,
        "a".repeat(64)
    );
    let profile = project.profile(&source, Vec::new());
    let plans = project.plan(&profile).unwrap();
    let artifacts = cargo_artifacts(&plans["rust"]);
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].filename, "serde-1.0.0.crate");
    assert_eq!(artifacts[0].metadata, serde_json::Value::Null);
}
