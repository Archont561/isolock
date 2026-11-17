//! Behavioral coverage for large-blob sharding and POSIX reassembly.

use isolock_cas::{
    read_shard_manifest, shard_file, verify_shards, Sha256Digest, ShardManifest, SHARD_SIZE_BYTES,
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(unix)]
use std::process::{Command, Stdio};

#[derive(Default)]
struct ShardingWorld {
    workspace: RefCell<Option<TestDirectory>>,
    expected_size: RefCell<Option<u64>>,
    expected_digest: RefCell<Option<Sha256Digest>>,
    manifest: RefCell<Option<ShardManifest>>,
    reassembled_path: RefCell<Option<PathBuf>>,
}

#[fixture]
fn world() -> ShardingWorld {
    ShardingWorld::default()
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "isolock-cas-sharding-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create unique test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[given("an empty artifact workspace with a 95 MiB shard ceiling")]
fn given_empty_artifact_workspace(world: &ShardingWorld) {
    *world.workspace.borrow_mut() = Some(TestDirectory::new());
}

#[when("an artifact one byte larger than 95 MiB is ingested")]
fn when_oversized_artifact_is_ingested(world: &ShardingWorld) {
    let workspace = world.workspace.borrow();
    let workspace = workspace.as_ref().expect("workspace initialized");
    let source_path = workspace.path().join("sample.bin");
    let shard_directory = workspace.path().join("shards");
    let expected_size = SHARD_SIZE_BYTES + 1;
    let mut source = File::create(&source_path).expect("create source artifact");
    let mut hasher = Sha256::new();
    let mut block = [0_u8; 64 * 1024];
    for (index, byte) in block.iter_mut().enumerate() {
        *byte = (index.wrapping_mul(31) % 251) as u8;
    }

    let mut remaining = expected_size;
    while remaining > 0 {
        let length = remaining.min(block.len() as u64) as usize;
        source
            .write_all(&block[..length])
            .expect("write source artifact");
        hasher.update(&block[..length]);
        remaining -= length as u64;
    }
    source.flush().expect("flush source artifact");
    drop(source);

    let expected_digest = Sha256Digest::parse(&hex::encode(hasher.finalize()))
        .expect("SHA-256 output is a valid digest");
    let manifest = shard_file(&source_path, &shard_directory)
        .expect("shard large artifact")
        .expect("artifact exceeds the shard ceiling");

    *world.expected_size.borrow_mut() = Some(expected_size);
    *world.expected_digest.borrow_mut() = Some(expected_digest);
    *world.manifest.borrow_mut() = Some(manifest);
}

#[then("the artifact is split into sequential part files")]
fn then_artifact_is_split_sequentially(world: &ShardingWorld) {
    let workspace = world.workspace.borrow();
    let workspace = workspace.as_ref().expect("workspace initialized");
    let manifest = world.manifest.borrow();
    let manifest = manifest.as_ref().expect("shard manifest created");

    assert_eq!(manifest.chunks.len(), 2);
    assert_eq!(manifest.chunks[0].index, 0);
    assert_eq!(manifest.chunks[0].part_filename, "sample.bin.part000");
    assert_eq!(manifest.chunks[0].size_bytes, SHARD_SIZE_BYTES);
    assert_eq!(manifest.chunks[1].index, 1);
    assert_eq!(manifest.chunks[1].part_filename, "sample.bin.part001");
    assert_eq!(manifest.chunks[1].size_bytes, 1);
    assert!(workspace
        .path()
        .join("shards")
        .join(&manifest.chunks[0].part_filename)
        .is_file());
    assert!(workspace
        .path()
        .join("shards")
        .join(&manifest.chunks[1].part_filename)
        .is_file());
}

#[then("the manifest records the total size and each part digest")]
fn then_manifest_records_sizes_and_digests(world: &ShardingWorld) {
    let workspace = world.workspace.borrow();
    let workspace = workspace.as_ref().expect("workspace initialized");
    let manifest = world.manifest.borrow();
    let manifest = manifest.as_ref().expect("shard manifest created");
    let expected_size = world
        .expected_size
        .borrow()
        .as_ref()
        .copied()
        .expect("expected size recorded");
    let expected_digest = world
        .expected_digest
        .borrow()
        .clone()
        .expect("expected digest recorded");
    let shard_directory = workspace.path().join("shards");

    assert_eq!(manifest.original_size_bytes, expected_size);
    assert_eq!(manifest.original_sha256, expected_digest);
    for part in &manifest.chunks {
        let (actual_size, actual_digest) =
            digest_file(&shard_directory.join(&part.part_filename)).expect("hash shard file");
        assert_eq!(actual_size, part.size_bytes);
        assert_eq!(actual_digest, part.sha256);
    }
    verify_shards(&shard_directory, manifest).expect("all shard and original digests verify");

    let manifest_path = shard_directory.join(manifest.manifest_filename());
    let serialized_manifest = read_shard_manifest(&manifest_path).expect("read written manifest");
    assert_eq!(&serialized_manifest, manifest);
}

#[when("the shards are reassembled using POSIX cat")]
fn when_shards_are_reassembled(world: &ShardingWorld) {
    let workspace = world.workspace.borrow();
    let workspace = workspace.as_ref().expect("workspace initialized");
    let manifest = world.manifest.borrow();
    let manifest = manifest.as_ref().expect("shard manifest created");
    let shard_directory = workspace.path().join("shards");
    let part_paths: Vec<PathBuf> = manifest
        .chunks
        .iter()
        .map(|chunk| shard_directory.join(&chunk.part_filename))
        .collect();
    let reassembled_path = workspace.path().join("reassembled.bin");

    concatenate_parts(&part_paths, &reassembled_path).expect("concatenate ordered shard files");
    *world.reassembled_path.borrow_mut() = Some(reassembled_path);
}

#[then("the reassembled payload matches the source and its SHA-256 digest")]
fn then_reassembled_payload_matches_source(world: &ShardingWorld) {
    let workspace = world.workspace.borrow();
    let workspace = workspace.as_ref().expect("workspace initialized");
    let source_path = workspace.path().join("sample.bin");
    let reassembled_path = world
        .reassembled_path
        .borrow()
        .clone()
        .expect("reassembled file created");
    let expected_size = world
        .expected_size
        .borrow()
        .as_ref()
        .copied()
        .expect("expected size recorded");
    let expected_digest = world
        .expected_digest
        .borrow()
        .clone()
        .expect("expected digest recorded");
    let (actual_size, actual_digest) =
        digest_file(&reassembled_path).expect("hash reassembled file");

    assert_eq!(actual_size, expected_size);
    assert_eq!(actual_digest, expected_digest);
    assert!(
        files_equal(&source_path, &reassembled_path).expect("compare source and reassembled files")
    );
}

#[then("a modified shard is rejected by verification")]
fn then_modified_shard_is_rejected(world: &ShardingWorld) {
    let workspace = world.workspace.borrow();
    let workspace = workspace.as_ref().expect("workspace initialized");
    let manifest = world.manifest.borrow();
    let manifest = manifest.as_ref().expect("shard manifest created");
    let shard_directory = workspace.path().join("shards");
    let last_part = shard_directory.join(&manifest.chunks[1].part_filename);

    fs::write(&last_part, b"\xff").expect("tamper with the final one-byte shard");
    let result = verify_shards(&shard_directory, manifest);
    assert!(matches!(
        result,
        Err(isolock_cas::CasError::DigestMismatch { .. })
    ));
}

#[cfg(unix)]
fn concatenate_parts(parts: &[PathBuf], destination: &Path) -> io::Result<()> {
    let output = File::create(destination)?;
    let mut command = Command::new("cat");
    for part in parts {
        command.arg(part);
    }
    let status = command.stdout(Stdio::from(output)).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            "POSIX cat failed while reassembling shards",
        ))
    }
}

#[cfg(not(unix))]
fn concatenate_parts(parts: &[PathBuf], destination: &Path) -> io::Result<()> {
    let mut output = File::create(destination)?;
    for part in parts {
        io::copy(&mut File::open(part)?, &mut output)?;
    }
    output.flush()
}

fn digest_file(path: &Path) -> io::Result<(u64, Sha256Digest)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        hasher.update(&buffer[..length]);
        size += length as u64;
    }
    let digest = Sha256Digest::parse(&hex::encode(hasher.finalize()))
        .expect("SHA-256 output is a valid digest");
    Ok((size, digest))
}

fn files_equal(left_path: &Path, right_path: &Path) -> io::Result<bool> {
    let mut left = File::open(left_path)?;
    let mut right = File::open(right_path)?;
    let mut left_buffer = [0_u8; 64 * 1024];
    let mut right_buffer = [0_u8; 64 * 1024];
    loop {
        let left_length = read_block(&mut left, &mut left_buffer)?;
        let right_length = read_block(&mut right, &mut right_buffer)?;
        if left_length != right_length || left_buffer[..left_length] != right_buffer[..right_length]
        {
            return Ok(false);
        }
        if left_length == 0 {
            return Ok(true);
        }
    }
}

fn read_block(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = reader.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

#[test]
fn artifact_at_shard_limit_remains_unsharded() {
    let workspace = TestDirectory::new();
    let source_path = workspace.path().join("limit.bin");
    let shard_directory = workspace.path().join("shards");
    let source = File::create(&source_path).expect("create sparse source file");
    source
        .set_len(SHARD_SIZE_BYTES)
        .expect("set source to exact shard limit");
    drop(source);

    assert!(shard_file(&source_path, &shard_directory)
        .expect("accept artifact at shard limit")
        .is_none());
    assert!(!shard_directory.exists());
}

#[scenario(path = "features/sharding.feature", index = 0)]
fn oversized_artifacts_are_split_verified_and_reassembled(world: ShardingWorld) {
    let _ = world;
}
