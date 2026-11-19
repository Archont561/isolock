#![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures fail fast on invalid setup.
#![cfg(unix)]

use isolock_cas::Sha256Digest;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const BOOTSTRAP_TEMPLATE: &str = include_str!("../../../templates/bootstrap.sh");
const COMMAND_NAME: &str = "isolock-fixture-command";

struct Fixture {
    root: PathBuf,
    temp_root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!(
            "isolock-bootstrap-test-{}-{nonce}",
            std::process::id()
        ));
        let root = temp_root.join("project with ' quote");
        fs::create_dir_all(root.join(".isolock")).expect("creates fixture project");
        fs::write(root.join(".isolock/bootstrap.sh"), BOOTSTRAP_TEMPLATE)
            .expect("installs bootstrap template into the fixture project");
        Self { root, temp_root }
    }

    fn add_archive(&self, bytes: &[u8], filename: &str, sharded: bool) -> Sha256Digest {
        let digest = Sha256Digest::from_bytes(bytes);
        let blob = self
            .root
            .join(".isolock/blobs")
            .join(digest.sharded_path())
            .join(filename);
        fs::create_dir_all(blob.parent().expect("CAS blob has a parent directory"))
            .expect("creates sharded CAS directory");
        if sharded {
            let split = bytes.len() / 2;
            fs::write(format!("{}.part000", blob.display()), &bytes[..split])
                .expect("writes first CAS shard");
            fs::write(format!("{}.part001", blob.display()), &bytes[split..])
                .expect("writes second CAS shard");
        } else {
            fs::write(&blob, bytes).expect("writes complete CAS blob");
        }
        let manifest_dir = self.root.join(".isolock/environments");
        fs::create_dir_all(&manifest_dir).expect("creates environment manifest directory");
        fs::write(
            manifest_dir.join("active.manifest"),
            format!(
                "# sha256 digest and archive filename\n{} {filename}\n",
                digest.as_str()
            ),
        )
        .expect("writes environment manifest");
        digest
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new("sh")
            .arg(self.root.join(".isolock/bootstrap.sh"))
            .args(arguments)
            .current_dir(&self.root)
            .output()
            .expect("runs the bootstrap script with /bin/sh")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temp_root);
    }
}

fn create_toolchain_archive(workspace: &Path) -> Vec<u8> {
    let source = workspace.join("archive-source");
    let bin = source.join("bin");
    fs::create_dir_all(&bin).expect("creates archive bin directory");
    let tool = bin.join(COMMAND_NAME);
    fs::write(&tool, "#!/bin/sh\nexit 0\n").expect("writes fixture executable");
    let mut permissions = fs::metadata(&tool)
        .expect("reads fixture executable permissions")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&tool, permissions).expect("marks fixture executable");

    let archive = workspace.join("toolchain.tar");
    let output = Command::new("tar")
        .arg("-cf")
        .arg(&archive)
        .arg("-C")
        .arg(&source)
        .arg(".")
        .output()
        .expect("creates fixture tar archive");
    assert!(
        output.status.success(),
        "tar failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read(archive).expect("reads fixture tar archive")
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .expect("runs git command");
    assert!(
        output.status.success(),
        "git {arguments:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bootstrap_verifies_unpacks_and_activates_local_cas_archives() {
    let fixture = Fixture::new();
    let archive = create_toolchain_archive(&fixture.temp_root);
    fixture.add_archive(&archive, "toolchain.tar", false);

    let output = fixture.run(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let environment = fixture.root.join(".isolock/state/active");
    assert_eq!(
        fs::read(environment.join("bin").join(COMMAND_NAME)).expect("tool was unpacked"),
        b"#!/bin/sh\nexit 0\n"
    );

    let activate = environment.join("activate");
    let activation = Command::new("sh")
        .arg("-c")
        .arg("PATH=; . \"$1\"; command -v isolock-fixture-command >/dev/null")
        .arg("bootstrap-activation-test")
        .arg(activate)
        .output()
        .expect("sources generated activation script");
    assert!(
        activation.status.success(),
        "activation failed: {}",
        String::from_utf8_lossy(&activation.stderr)
    );
}

#[test]
fn bootstrap_reassembles_ordered_cas_shards_and_checks_the_full_digest() {
    let fixture = Fixture::new();
    let archive = create_toolchain_archive(&fixture.temp_root);
    fixture.add_archive(&archive, "toolchain.tar", true);

    let output = fixture.run(&["active"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout)
        .contains("Reassembled toolchain.tar from 2 shard(s)"));
    assert!(fixture
        .root
        .join(".isolock/state/active/bin")
        .join(COMMAND_NAME)
        .is_file());
}

#[test]
fn bootstrap_rejects_a_blob_that_does_not_match_the_manifest_digest() {
    let fixture = Fixture::new();
    let archive = create_toolchain_archive(&fixture.temp_root);
    let digest = fixture.add_archive(&archive, "toolchain.tar", false);
    let blob = fixture
        .root
        .join(".isolock/blobs")
        .join(digest.sharded_path())
        .join("toolchain.tar");
    fs::write(blob, b"tampered archive").expect("corrupts the CAS blob");

    let output = fixture.run(&[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SHA-256 mismatch"));
    assert!(!fixture.root.join(".isolock/state/active").exists());
}

#[test]
#[allow(clippy::too_many_lines)] // End-to-end fixture setup and lifecycle assertions stay together.
fn bootstrap_remote_mode_fetches_storage_tree_without_checking_it_out() {
    let fixture = Fixture::new();
    let archive = create_toolchain_archive(&fixture.temp_root);
    let digest = Sha256Digest::from_bytes(&archive);
    let storage = fixture.temp_root.join("storage-worktree");
    let remote = fixture.temp_root.join("storage-origin.git");
    fs::create_dir_all(&storage).expect("creates storage worktree");
    git(&storage, &["init", "--quiet"]);
    git(&storage, &["config", "user.name", "Isolock Test"]);
    git(
        &storage,
        &["config", "user.email", "isolock-test@example.invalid"],
    );
    git(&storage, &["checkout", "--quiet", "--orphan", "isolock/v1"]);

    let blob = storage
        .join("blobs")
        .join(digest.sharded_path())
        .join("toolchain.tar");
    fs::create_dir_all(blob.parent().expect("storage blob has a parent directory"))
        .expect("creates remote CAS directory");
    fs::write(blob, &archive).expect("writes remote archive");
    let environments = storage.join("environments");
    fs::create_dir_all(&environments).expect("creates remote manifest directory");
    fs::write(
        environments.join("active.manifest"),
        format!("{} toolchain.tar\n", digest.as_str()),
    )
    .expect("writes remote environment manifest");
    git(&storage, &["add", "--all"]);
    git(
        &storage,
        &["commit", "--quiet", "-m", "Initialize Isolock storage"],
    );
    git(
        &storage,
        &[
            "init",
            "--bare",
            "--quiet",
            remote.to_str().expect("remote path is UTF-8"),
        ],
    );
    git(
        &storage,
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().expect("remote path is UTF-8"),
        ],
    );
    git(
        &storage,
        &["push", "--quiet", "origin", "HEAD:refs/heads/isolock/v1"],
    );

    git(&fixture.root, &["init", "--quiet"]);
    git(
        &fixture.root,
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().expect("remote path is UTF-8"),
        ],
    );
    let bootstrap = fixture.root.join(".isolock/bootstrap.sh");
    fs::write(&bootstrap, BOOTSTRAP_TEMPLATE).expect("restores bootstrap template after git init");

    let output = fixture.run(&["--remote"]);
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture
        .root
        .join(".isolock/state/active/bin")
        .join(COMMAND_NAME)
        .is_file());
    assert!(fixture.root.join(".isolock/blobs").is_dir());
}
