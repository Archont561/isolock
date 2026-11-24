//! Profile-aware `exec`: a session runs inside the restored profile, or fails closed.
//!
//! The assertions use a fake toolchain that reports the `PATH` and `CARGO_HOME` it was
//! given, because the question is what the child receives, not what the tool does with it.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Fixtures fail fast on invalid setup.

use isolock_cas::{digest_file, FileStore};
use isolock_plan::profile::ArchiveFormat;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const PLATFORM: &str = "linux-64";
const PROFILE: &str = "rust-standalone";

/// Reports the environment a profile session must provide, then succeeds.
const TOOL: &str = r#"#!/bin/sh
set -eu
[ -n "${CARGO_HOME:-}" ] || { echo "no CARGO_HOME" >&2; exit 1; }
case "$PATH" in
  *"/.isolock/profiles/"*) ;;
  *) echo "profile prefix is not on PATH: $PATH" >&2; exit 1 ;;
esac
# The proxy must actually serve the profile's registry artifact, not an empty snapshot.
index="${CARGO_REGISTRIES_CRATES_IO_INDEX#sparse+}"
[ -n "$index" ] || { echo "no registry index configured" >&2; exit 1; }
cfg=$(curl -fsS "${index}config.json" 2>/dev/null || true)
case "$cfg" in
  *download*) ;;
  *) echo "proxy config.json missing: $cfg" >&2; exit 1 ;;
esac
body=$(curl -fsS "${index}se/rd/serde" 2>/dev/null || true)
case "$body" in
  *serde*) ;;
  *) echo "proxy does not serve serde: [$body] cfg=[$cfg]" >&2; exit 1 ;;
esac
printf 'ran from %s\n' "$(dirname "$0")"
"#;

/// Writes a minimal `.crate`: a gzipped tar with a `serde-1.0.0/Cargo.toml` root.
fn crate_archive(path: &Path) {
    let staging = tempfile::tempdir().unwrap();
    let root = staging.path().join("serde-1.0.0");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"serde\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn marker() {}\n").unwrap();
    let output = Command::new("tar")
        .arg("-czf")
        .arg(path)
        .arg("-C")
        .arg(staging.path())
        .arg("serde-1.0.0")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn archive(path: &Path, entries: &[(&str, &str)]) {
    let staging = tempfile::tempdir().unwrap();
    let mut tar = Command::new("tar");
    tar.arg("-czf").arg(path).arg("-C").arg(staging.path());
    for (entry, contents) in entries {
        let file = staging.path().join(entry);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, contents).unwrap();
        let mut permissions = fs::metadata(&file).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&file, permissions).unwrap();
        tar.arg(entry);
    }
    assert!(tar.status().unwrap().success());
}

struct World {
    temp: TempDir,
}

impl World {
    fn new() -> Self {
        Self {
            temp: tempfile::tempdir().unwrap(),
        }
    }
    fn root(&self) -> PathBuf {
        self.temp.path().join("project")
    }

    /// Writes a profile whose runtime prefix provides `cargo`, then plans and syncs it.
    #[allow(clippy::too_many_lines)] // A reviewed fixture reads as one sequence.
    fn materialize(&self) {
        let root = self.root();
        fs::create_dir_all(&root).unwrap();
        let tools = self.temp.path().join("tools");
        fs::create_dir_all(&tools).unwrap();
        // A `rust` runtime must provide both toolchain binaries, so the probe tool
        // rides along as `cargo`.
        archive(
            &tools.join("rust.tar.gz"),
            &[
                ("prefix/bin/cargo", TOOL),
                ("prefix/bin/rustc", "#!/bin/sh\nexit 0\n"),
            ],
        );
        archive(
            &tools.join("isolock.tar.gz"),
            &[("isolock/bin/isolock", "#!/bin/sh\nexit 0\n")],
        );
        let rust = digest_file(&tools.join("rust.tar.gz")).unwrap();
        let isolock = digest_file(&tools.join("isolock.tar.gz")).unwrap();
        // A real `.crate` (a gzipped tar), because the planner verifies the archive
        // before it will plan the artifact.
        let crate_bytes = self.temp.path().join("serde-1.0.0.crate");
        crate_archive(&crate_bytes);
        let crate_digest = digest_file(&crate_bytes).unwrap();
        let cas = root.join("cas");
        for (source, digest) in [
            (&tools.join("rust.tar.gz"), &rust),
            (&tools.join("isolock.tar.gz"), &isolock),
            (&crate_bytes, &crate_digest),
        ] {
            let store = FileStore::new(cas.clone());
            store.put_file(digest, source).unwrap();
        }
        // A locked Cargo dependency, so the session projection has a registry artifact
        // to serve rather than an empty snapshot.
        fs::write(
            root.join("Cargo.lock"),
            format!(
                "version = 4\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{}\"\n",
                crate_digest.as_str()
            ),
        )
        .unwrap();
        fs::write(
            root.join("isolock.toml"),
            format!(
                r#"schema-version = 1

[storage]
cas-dir = "cas"

[profiles.{PROFILE}]
provider = "standalone"
platforms = ["{PLATFORM}"]
cargo-lock = "Cargo.lock"

[profiles.{PROFILE}.platform-tools.{PLATFORM}.runtimes.rust]
version = "1.0.0"
platform = "{PLATFORM}"
digest = "{}"
filename = "rust.tar.gz"
source = "https://example.test/rust.tar.gz"
format = "tar-gz"
entry = "prefix"

[profiles.{PROFILE}.cargo]
cargo-version = "{}"
target = "x86_64-unknown-linux-gnu"

[profiles.{PROFILE}.platform-tools.{PLATFORM}.seeds.isolock]
version = "1.0.0"
platform = "{PLATFORM}"
digest = "{}"
filename = "isolock.tar.gz"
source = "https://example.test/isolock.tar.gz"
format = "tar-gz"
entry = "isolock/bin/isolock"
"#,
                rust.as_str(),
                "1.90.0",
                isolock.as_str()
            ),
        )
        .unwrap();
        self.success(&["plan", "--profile", PROFILE, "--platform", PLATFORM]);
        self.success(&["sync", "--profile", PROFILE, "--platform", PLATFORM]);
        let _ = ArchiveFormat::Tar;
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .current_dir(self.root())
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", self.temp.path())
            .args(arguments);
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().unwrap()
    }

    fn success(&self, arguments: &[&str]) -> Output {
        let output = self.run(arguments);
        assert!(output.status.success(), "{output:?}");
        output
    }
}

#[test]
fn exec_runs_a_child_inside_the_selected_profile() {
    let world = World::new();
    world.materialize();

    let output = world.success(&[
        "exec",
        "--profile",
        PROFILE,
        "--platform",
        PLATFORM,
        "--",
        "cargo",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The profile's `cargo` is a generated launcher that execs the verified runtime
    // binary, so the child reports a path inside `.isolock`, never a host toolchain.
    assert!(
        stdout.contains("/.isolock/runtimes/") || stdout.contains("/.isolock/profiles/"),
        "child did not run from the verified tree: {stdout}"
    );
}

#[test]
fn exec_without_a_profile_is_unchanged() {
    let world = World::new();
    fs::create_dir_all(world.root()).unwrap();
    // No configuration at all: the legacy path never requires a profile.
    let output = world.success(&["exec", "--", "true"]);
    assert!(output.stdout.is_empty(), "{output:?}");
}

#[test]
fn an_unmaterialized_profile_fails_closed_instead_of_using_the_host_toolchain() {
    let world = World::new();
    world.materialize();
    // Remove the restored prefix the same way an interrupted restore would.
    let profiles = world.root().join(".isolock/profiles");
    for entry in fs::read_dir(&profiles).unwrap().flatten() {
        for platform in fs::read_dir(entry.path()).unwrap().flatten() {
            for digest in fs::read_dir(platform.path()).unwrap().flatten() {
                fs::remove_dir_all(digest.path()).unwrap();
            }
        }
    }

    let output = world.run(&[
        "exec",
        "--profile",
        PROFILE,
        "--platform",
        PLATFORM,
        "--",
        "cargo",
    ]);
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("is not materialized"), "{stderr}");
    assert!(stderr.contains("isolock sync"), "{stderr}");
}
