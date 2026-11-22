//! Public profile/manifest validation boundaries, independent of environment and transport.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid contract fixtures must fail tests.

use isolock_cas::Sha256Digest;
use isolock_plan::profile::{
    build_profiles, canonical_digest, verify_integrity, ArchiveFormat, CargoDerivation,
    PlanRequest, PlatformTools, Profile, ProfileManifest, Provider, StorageIdentity, ToolArtifact,
};
use std::collections::BTreeMap;
use std::path::Path;

const PLATFORM: &str = "linux-64";

fn profile() -> Profile {
    Profile {
        platforms: vec![PLATFORM.into()],
        seeds: BTreeMap::from([(
            "isolock".into(),
            ToolArtifact {
                version: "1.0.0".into(),
                platform: PLATFORM.into(),
                digest: Sha256Digest::from_bytes(b"seed"),
                filename: "isolock".into(),
                source: "https://example.invalid/isolock".into(),
                format: ArchiveFormat::Raw,
                entry: None,
            },
        )]),
        ..Profile::default()
    }
}

fn manifest(storage: StorageIdentity) -> ProfileManifest {
    let profiles = BTreeMap::from([("test".into(), profile())]);
    build_profiles(&PlanRequest {
        root: Path::new("."),
        profile: "test",
        platform: PLATFORM,
        profiles: &profiles,
        storage,
    })
    .unwrap()
    .remove("test")
    .unwrap()
}

#[test]
fn profile_provider_and_tool_constraints_are_checked_before_any_transfer() {
    type Mutation = (fn(&mut Profile), &'static str);
    let cases: [Mutation; 12] = [
        (|p| p.platforms.clear(), "does not support platform"),
        (|p| p.seeds.clear(), "leaf profiles require"),
        (|p| p.compose.push("child".into()), "leaf profiles require"),
        (
            |p| p.provider = Provider::Composed,
            "composed profiles must only",
        ),
        (|p| p.provider = Provider::Pixi, "Pixi profiles require"),
        (
            |p| p.environment = Some("default".into()),
            "only Pixi profiles",
        ),
        (
            |p| p.cargo_lock = Some("Cargo.lock".into()),
            "explicit cargo-version and target",
        ),
        (
            |p| p.seeds.get_mut("isolock").unwrap().entry = Some("bin/isolock".into()),
            "raw tools cannot",
        ),
        (
            |p| p.seeds.get_mut("isolock").unwrap().format = ArchiveFormat::Tar,
            "requires an executable entry",
        ),
        (
            |p| p.seeds.get_mut("isolock").unwrap().platform = "osx-arm64".into(),
            "incompatible platform",
        ),
        (
            |p| p.seeds.get_mut("isolock").unwrap().version = "bad version".into(),
            "missing version",
        ),
        (
            |p| {
                let tool = p.seeds["isolock"].clone();
                p.seeds.insert("unknown".into(), tool);
            },
            "unsupported seed",
        ),
    ];
    profile().validate("test", PLATFORM).unwrap();
    for (mutate, expected) in cases {
        let mut candidate = profile();
        mutate(&mut candidate);
        let error = candidate.validate("test", PLATFORM).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn profile_resolution_rejects_undeclared_platform_tools_and_validates_archive_entries() {
    let mut candidate = profile();
    candidate
        .platform_tools
        .insert("osx-arm64".into(), PlatformTools::default());
    assert!(candidate
        .resolve(PLATFORM)
        .unwrap_err()
        .contains("undeclared platform"));
    candidate.platform_tools.clear();
    candidate
        .platform_tools
        .insert(PLATFORM.into(), PlatformTools::default());
    candidate.validate("test", PLATFORM).unwrap();
    assert!(candidate
        .resolve(PLATFORM)
        .unwrap()
        .platform_tools
        .is_empty());
    let tool = candidate.seeds.get_mut("isolock").unwrap();
    tool.format = ArchiveFormat::Tar;
    tool.entry = Some("../isolock".into());
    assert!(candidate
        .validate("test", PLATFORM)
        .unwrap_err()
        .contains("without traversal"));
}

#[test]
fn manifests_validate_schema_storage_destinations_and_derivation_identity() {
    type Mutation = (fn(&mut ProfileManifest), &'static str);
    let cases: [Mutation; 9] = [
        (
            |m| m.schema_version = 99,
            "unsupported profile manifest schema",
        ),
        (
            |m| m.storage.kind = "unreviewed".into(),
            "unsupported storage provider",
        ),
        (
            |m| m.storage.kind = "git".into(),
            "immutable storage commit",
        ),
        (
            |m| m.storage.repository = Some("https://token@example.invalid/repo".into()),
            "credentials",
        ),
        (
            |m| {
                m.inputs
                    .insert("../escape".into(), Sha256Digest::from_bytes(b"x"));
            },
            "without traversal",
        ),
        (
            |m| m.artifacts[0].destinations = vec!["elsewhere/file".into()],
            "inside node_modules",
        ),
        (
            |m| {
                m.materializers.insert("seeds".into(), 2);
            },
            "unsupported materializer schema",
        ),
        (
            |m| m.configuration_digest = Sha256Digest::from_bytes(b"different"),
            "configuration digest mismatch",
        ),
        (
            |m| {
                m.cargo_payloads.insert(
                    "wrong-key".into(),
                    CargoDerivation {
                        schema_version: 1,
                        lock_digest: Sha256Digest::from_bytes(b"lock"),
                        platform: PLATFORM.into(),
                        policy: Default::default(),
                    },
                );
            },
            "derivation digest mismatch",
        ),
    ];
    let original = manifest(StorageIdentity::default());
    for (mutate, expected) in cases {
        let mut candidate = original.clone();
        mutate(&mut candidate);
        let error = candidate.validate().unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn git_storage_requires_a_full_immutable_hexadecimal_commit() {
    for commit in ["short".to_owned(), "z".repeat(40)] {
        let mut candidate = manifest(StorageIdentity::default());
        candidate.storage = StorageIdentity {
            kind: "git".into(),
            repository: Some("https://example.invalid/repo".into()),
            reference: Some("main".into()),
            commit: Some(commit),
        };
        assert!(candidate
            .validate()
            .unwrap_err()
            .contains("immutable storage commit"));
    }
    for length in [40, 64] {
        let candidate = manifest(StorageIdentity {
            kind: "git".into(),
            repository: Some("https://example.invalid/repo".into()),
            reference: Some("main".into()),
            commit: Some("a".repeat(length)),
        });
        candidate.validate().unwrap();
    }
}

#[test]
fn integrity_validation_distinguishes_sha256_sri_syntax_and_sri_digest_failures() {
    let mut artifact = manifest(StorageIdentity::default()).artifacts.remove(0);
    artifact.digest = Sha256Digest::from_bytes(b"hello");
    assert_eq!(
        verify_integrity(&artifact, b"wrong").unwrap_err(),
        "artifact SHA-256 mismatch"
    );
    artifact.integrity = Some("not-an-integrity-value".into());
    assert!(verify_integrity(&artifact, b"hello")
        .unwrap_err()
        .contains("invalid locked SRI"));
    artifact.integrity = Some("sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into());
    assert!(verify_integrity(&artifact, b"hello")
        .unwrap_err()
        .contains("integrity mismatch"));
    artifact.integrity = Some("sha256-LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=".into());
    verify_integrity(&artifact, b"hello").unwrap();
}

#[test]
fn canonical_hashing_propagates_serialization_failure_instead_of_hashing_partial_data() {
    struct Unserializable;
    impl serde::Serialize for Unserializable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("fixture cannot serialize"))
        }
    }
    assert!(canonical_digest(&Unserializable)
        .unwrap_err()
        .contains("fixture cannot serialize"));
}
