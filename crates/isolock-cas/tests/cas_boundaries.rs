//! Fail-closed manifest validation, CAS filesystem boundaries, and error diagnostics.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid fixtures must fail the test.

use isolock_cas::{
    read_shard_manifest, shard_file, verify_shards, CasError, FileStore, Sha256Digest,
    ShardManifest, ShardPart, SHARD_MANIFEST_SCHEMA_VERSION, SHARD_SIZE_BYTES,
};
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, Read};

fn manifest() -> ShardManifest {
    ShardManifest {
        schema_version: SHARD_MANIFEST_SCHEMA_VERSION,
        original_filename: "archive.bin".into(),
        original_size_bytes: SHARD_SIZE_BYTES + 1,
        original_sha256: Sha256Digest::from_bytes(b"original"),
        chunk_size_bytes: SHARD_SIZE_BYTES,
        chunks: vec![
            ShardPart {
                index: 0,
                part_filename: "archive.bin.part000".into(),
                size_bytes: SHARD_SIZE_BYTES,
                sha256: Sha256Digest::from_bytes(b"first"),
            },
            ShardPart {
                index: 1,
                part_filename: "archive.bin.part001".into(),
                size_bytes: 1,
                sha256: Sha256Digest::from_bytes(b"last"),
            },
        ],
    }
}

#[test]
fn malformed_manifests_are_rejected_before_any_part_is_opened() {
    type Mutation = (fn(&mut ShardManifest), &'static str);
    let cases: [Mutation; 8] = [
        (|m| m.schema_version = 2, "unsupported schema version"),
        (
            |m| m.original_size_bytes = SHARD_SIZE_BYTES,
            "larger than the shard ceiling",
        ),
        (|m| m.chunk_size_bytes = 1, "chunk size must be"),
        (
            |m| {
                m.chunks.pop();
            },
            "expected 2 parts but found 1",
        ),
        (|m| m.chunks[0].index = 1, "position 0 has index 1"),
        (
            |m| m.chunks[0].part_filename = "../outside".into(),
            "must be named archive.bin.part000",
        ),
        (
            |m| m.chunks[1].size_bytes = 2,
            "must contain 1 bytes, not 2",
        ),
        (
            |m| m.original_filename = "../outside".into(),
            "not a safe basename",
        ),
    ];
    manifest().validate().unwrap();
    for (mutate, expected) in cases {
        let mut candidate = manifest();
        mutate(&mut candidate);
        let error = verify_shards("a-directory-that-does-not-exist", &candidate).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        assert!(error.source().is_none());
    }
}

#[test]
fn portable_manifest_filenames_reject_empty_dot_traversal_and_nul() {
    for filename in [
        "",
        ".",
        "..",
        "nested/archive",
        "nested\\archive",
        "bad\0name",
    ] {
        let mut candidate = manifest();
        candidate.original_filename = filename.into();
        assert!(matches!(
            candidate.validate(),
            Err(CasError::InvalidFilename(_))
        ));
    }
}

#[test]
fn truncated_shards_report_expected_and_actual_sizes() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("archive.bin.part000"), b"short").unwrap();
    let error = verify_shards(directory.path(), &manifest()).unwrap_err();
    assert!(
        matches!(&error, CasError::SizeMismatch { expected, actual: 5, .. } if *expected == SHARD_SIZE_BYTES)
    );
    let message = error.to_string();
    assert!(message.contains("archive.bin.part000"));
    assert!(message.contains("found 5"));
}

#[test]
fn io_and_json_errors_preserve_their_cause_without_publishing_output() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("shards");
    let error = shard_file(directory.path().join("missing"), &destination).unwrap_err();
    assert!(error.to_string().contains("CAS I/O error"));
    assert!(error.source().unwrap().is::<io::Error>());
    assert!(!destination.exists());
    let path = directory.path().join("manifest.json");
    fs::write(&path, b"not JSON").unwrap();
    let error = read_shard_manifest(path).unwrap_err();
    assert!(error.to_string().contains("invalid shard manifest JSON"));
    assert!(error.source().unwrap().is::<serde_json::Error>());
}

#[test]
fn error_diagnostics_identify_the_artifact_and_changed_source_sizes() {
    let error = CasError::SourceChanged {
        path: "input.bin".into(),
        expected: 10,
        actual: 9,
    };
    assert_eq!(
        error.to_string(),
        "source input.bin changed while sharding (expected 10 bytes, read 9)"
    );
    assert!(error.source().is_none());
    let digest = Sha256Digest::from_bytes(b"expected");
    let actual = Sha256Digest::from_bytes(b"actual");
    let error = CasError::DigestMismatch {
        artifact: "input.bin".into(),
        expected: digest.clone(),
        actual: actual.clone(),
    };
    assert_eq!(
        error.to_string(),
        format!(
            "SHA-256 mismatch for input.bin: expected {}, found {}",
            digest.as_str(),
            actual.as_str()
        )
    );
    assert!(error.source().is_none());
    assert!(Sha256Digest::parse(&"x".repeat(64))
        .unwrap_err()
        .contains("hexadecimal"));
}

#[test]
fn failed_incoming_streams_leave_no_blob_or_staged_file() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "fixture read failure",
            ))
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let store = FileStore::new(directory.path());
    let digest = Sha256Digest::from_bytes(b"expected");
    let error = store.put_reader(&digest, &mut Broken).unwrap_err();
    assert!(error.to_string().contains("fixture read failure"));
    assert!(!store.path(&digest).exists());
    assert!(store.digests().unwrap().is_empty());
    assert_eq!(
        fs::read_dir(store.path(&digest).parent().unwrap())
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn enumeration_refuses_noncanonical_directory_names_and_blob_locations() {
    for prefix in ["A0", "abc", "gg", "a"] {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("sha256").join(prefix)).unwrap();
        assert!(matches!(
            FileStore::new(directory.path()).digests(),
            Err(CasError::InvalidSource(_))
        ));
    }
    for leaf in ["invalid".to_owned(), "A".repeat(64), "0".repeat(64)] {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("sha256/ab/cd");
        fs::create_dir_all(&parent).unwrap();
        fs::write(parent.join(leaf), b"not a canonical CAS entry").unwrap();
        assert!(FileStore::new(directory.path()).digests().is_err());
    }
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("sha256")).unwrap();
    fs::write(directory.path().join("sha256/ab"), b"not a directory").unwrap();
    assert!(FileStore::new(directory.path()).digests().is_err());
}

#[cfg(unix)]
#[test]
fn nonregular_sources_and_non_utf8_filenames_are_rejected_before_sharding() {
    use std::os::unix::ffi::OsStringExt;
    let directory = tempfile::tempdir().unwrap();
    let error = shard_file(directory.path(), directory.path().join("shards")).unwrap_err();
    assert!(matches!(error, CasError::InvalidSource(_)));
    assert!(error.to_string().contains("not a regular file"));
    let path = directory
        .path()
        .join(std::ffi::OsString::from_vec(vec![0xFF]));
    File::create(&path)
        .unwrap()
        .set_len(SHARD_SIZE_BYTES + 1)
        .unwrap();
    assert!(matches!(
        shard_file(path, directory.path().join("shards")),
        Err(CasError::InvalidFilename(_))
    ));
    assert!(!directory.path().join("shards").exists());
}

#[cfg(unix)]
#[test]
fn symlinks_and_directories_are_never_treated_as_verified_blobs() {
    use std::os::unix::fs::symlink;
    for symbolic in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let store = FileStore::new(directory.path().join("cas"));
        let digest = Sha256Digest::from_bytes(b"outside bytes");
        let path = store.path(&digest);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let outside = directory.path().join("outside");
        fs::write(&outside, b"outside bytes").unwrap();
        if symbolic {
            symlink(&outside, &path).unwrap();
        } else {
            fs::create_dir(&path).unwrap();
        }
        assert!(matches!(
            store.verified(&digest),
            Err(CasError::InvalidSource(_))
        ));
        assert!(store.put_bytes(&digest, b"outside bytes").is_err());
        assert!(store.digests().is_err());
        assert_eq!(fs::read(outside).unwrap(), b"outside bytes");
    }
}

#[cfg(unix)]
#[test]
fn enumeration_rejects_non_utf8_blob_names() {
    use std::os::unix::ffi::OsStringExt;
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("sha256/ab/cd");
    fs::create_dir_all(&parent).unwrap();
    fs::write(
        parent.join(std::ffi::OsString::from_vec(vec![0xFF])),
        b"invalid",
    )
    .unwrap();
    assert!(matches!(
        FileStore::new(directory.path()).digests(),
        Err(CasError::InvalidSource(_))
    ));
}

#[cfg(unix)]
#[test]
fn unreadable_cas_paths_are_errors_not_cache_misses_or_empty_gc_candidates() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let store = FileStore::new(directory.path().join("cas"));
    let digest = Sha256Digest::from_bytes(b"expected");
    store.put_bytes(&digest, b"expected").unwrap();
    let parent = store.path(&digest).parent().unwrap().to_path_buf();
    for blocked in [&parent, &store.root().join("sha256"), store.root()] {
        let permissions = fs::metadata(blocked).unwrap().permissions();
        fs::set_permissions(blocked, fs::Permissions::from_mode(0o000)).unwrap();
        let privileged = fs::read_dir(blocked).is_ok();
        let verified = store.verified(&digest);
        let candidates = store.digests();
        fs::set_permissions(blocked, permissions).unwrap();
        if privileged {
            eprintln!("permission boundary is unavailable to this privileged test process");
            continue;
        }
        assert!(
            matches!(verified, Err(CasError::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied)
        );
        assert!(
            matches!(candidates, Err(CasError::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied)
        );
    }
    assert!(store.verified(&digest).unwrap());
}
