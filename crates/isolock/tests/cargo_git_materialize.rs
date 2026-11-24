//! A planned Cargo git dependency materialises into a buildable vendored tree.
//!
//! The assertions run against a fake `cargo` that inspects what restoration
//! produced, because the question is *what we hand Cargo*, not whether Cargo
//! compiles. Real `git` produces the commit archive, so the tree itself is real.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Fixtures fail fast on invalid setup.

use isolock_cas::{digest_file, FileStore, Sha256Digest};
use isolock_plan::profile::{ArchiveFormat, ArtifactKind};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const PLATFORM: &str = "linux-64";
const PROFILE: &str = "rust-standalone";

/// A fake toolchain that reports what it was given, then succeeds.
///
/// It fails the run if any expectation does not hold, so a wrong vendor layout
/// surfaces as a failed build rather than a silently passing test.
const CARGO: &str = r#"#!/bin/sh
set -eu
[ "${CARGO_NET_OFFLINE:-}" = true ] || { echo "not offline" >&2; exit 1; }
config="$CARGO_HOME/config.toml"
grep -q 'replace-with = "isolock-vendor"' "$config" || { echo "no vendor source" >&2; exit 1; }
grep -q '^\[source\."git+' "$config" || { echo "no git source stanza" >&2; exit 1; }
grep -q '^git = ' "$config" || { echo "git source is not a git stanza" >&2; exit 1; }
if grep -q 'registry = "git+' "$config"; then echo "git url used as registry" >&2; exit 1; fi
vendor="$(sed -n 's/^directory = "\(.*\)"$/\1/p' "$config")"
[ -n "$vendor" ] || { echo "no vendor directory" >&2; exit 1; }
count=0
for package in "$vendor"/*/; do
  [ -f "$package/.cargo-checksum.json" ] || { echo "missing checksum: $package" >&2; exit 1; }
  [ -f "$package/Cargo.toml" ] || { echo "missing crate root: $package" >&2; exit 1; }
  count=$((count + 1))
done
[ "$count" -eq 1 ] || { echo "expected one vendored package, found $count" >&2; exit 1; }
printf 'cargo fixture: one vendored git dependency\n'
"#;

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

/// Builds a `.tar.gz` holding several files, the format the materialiser expects.
fn tool_archive_many(path: &Path, entries: &[(&str, &str)]) {
    let staging = tempfile::tempdir().unwrap();
    let mut tar = Command::new("tar");
    tar.arg("-czf").arg(path).arg("-C").arg(staging.path());
    for (entry, contents) in entries {
        let file = staging.path().join(entry);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, contents).unwrap();
        // The materialiser requires executables in a verified prefix.
        let mut permissions = fs::metadata(&file).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        fs::set_permissions(&file, permissions).unwrap();
        tar.arg(entry);
    }
    let output = tar.output().unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn tool_archive(path: &Path, entry: &str, contents: &str) {
    tool_archive_many(path, &[(entry, contents)]);
}

impl World {
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

    fn root(&self) -> PathBuf {
        self.temp.path().join("project")
    }

    fn upstream(&self) -> PathBuf {
        self.temp.path().join("upstream")
    }

    /// The digest `git archive` produces for the pinned commit.
    fn archive_digest(&self) -> Sha256Digest {
        let archive = self.temp.path().join("exported.tar");
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

    fn run(&self, arguments: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", self.temp.path().join("empty-home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .current_dir(self.root())
            .args(arguments);
        if let Some(value) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", value);
        }
        command.output().unwrap()
    }

    fn success(&self, arguments: &[&str]) -> Output {
        let output = self.run(arguments);
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    /// Writes a standalone Rust profile whose lockfile names one git dependency.
    fn configure(&self) {
        let root = self.root();
        fs::create_dir_all(root.join("src")).unwrap();
        let source = format!("file://{}", self.upstream().to_str().unwrap());
        fs::write(
            root.join("Cargo.lock"),
            format!(
                "version = 4\n\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\"gitdep\"]\n\n[[package]]\nname = \"gitdep\"\nversion = \"0.1.0\"\nsource = \"git+{source}?rev={commit}#{commit}\"\n",
                commit = self.commit
            ),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ngitdep = {{ git = \"{source}\", rev = \"{}\" }}\n",
                self.commit
            ),
        )
        .unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"{}\", gitdep::answer()); }\n",
        )
        .unwrap();

        // Tools are reviewed artifacts in CAS, exactly like a registry crate.
        let tools = self.temp.path().join("tools");
        fs::create_dir(&tools).unwrap();
        // One runtime prefix must provide both binaries the profile expects.
        tool_archive_many(
            &tools.join("rust.tar.gz"),
            &[
                ("prefix/bin/cargo", CARGO),
                ("prefix/bin/rustc", "#!/bin/sh\nexit 0\n"),
            ],
        );
        tool_archive(
            &tools.join("isolock.tar.gz"),
            "isolock/bin/isolock",
            "#!/bin/sh\nexit 0\n",
        );
        let cas = FileStore::new(root.join(".isolock/cas"));
        // One `rust` runtime archive provides the toolchain; `isolock` is the seed.
        let mut runtime_entries = String::new();
        let mut seed_entries = String::new();
        for (name, filename, target) in [
            ("rust", "rust.tar.gz", &mut runtime_entries),
            ("isolock", "isolock.tar.gz", &mut seed_entries),
        ] {
            let path = tools.join(filename);
            let digest = digest_file(&path).unwrap();
            cas.put_file(&digest, &path).unwrap();
            let digest = digest.as_str();
            use std::fmt::Write as _;
            writeln!(
                target,
                "{name} = {{ version = \"1.0.0\", platform = \"{PLATFORM}\", digest = \"{digest}\", filename = \"{filename}\", source = \"https://example.test/{filename}\", format = \"tar-gz\"{entry} }}",
                entry = if name == "isolock" { ", entry = \"isolock/bin/isolock\"" } else { ", entry = \"prefix\"" }
            )
            .unwrap();
        }
        let _ = ArchiveFormat::TarGz;

        let digest = self.archive_digest();
        let digest = digest.as_str();
        fs::write(
            root.join("isolock.toml"),
            format!(
                "schema-version = 1\n\n[profiles.{PROFILE}]\nprovider = \"standalone\"\nplatforms = [\"{PLATFORM}\"]\ncargo-lock = \"Cargo.lock\"\n\n[profiles.{PROFILE}.cargo]\ncargo-version = \"1.88.0\"\ntarget = \"x86_64-unknown-linux-gnu\"\nall-features = true\n\n[[profiles.{PROFILE}.cargo-git]]\nname = \"gitdep\"\nversion = \"0.1.0\"\nrepository = \"{source}\"\ncommit = \"{commit}\"\ndigest = \"{digest}\"\n\n[profiles.{PROFILE}.platform-tools.{PLATFORM}.seeds]\n{seed_entries}\n[profiles.{PROFILE}.platform-tools.{PLATFORM}.runtimes]\n{runtime_entries}",
                commit = self.commit
            ),
        )
        .unwrap();
    }

    /// Ingests the pinned commit, which is what puts its bytes into CAS.
    fn ingest(&self) {
        let source = format!("file://{}", self.upstream().to_str().unwrap());
        fs::write(
            self.root().join("isolock.ingest.json"),
            serde_json::to_vec_pretty(&json!({
                "schema-version": 1,
                "artifacts": [{
                    "registry": "cargo",
                    "name": "gitdep",
                    "version": "0.1.0",
                    "sha256": self.archive_digest(),
                    "filename": "gitdep-git.tar",
                    "source": source,
                    "git": {
                        "repository": source,
                        "revision": self.commit,
                        "commit": self.commit
                    }
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::create_dir_all(self.root().join("originals")).unwrap();
        self.success(&[
            "ingest",
            "--manifest",
            "isolock.ingest.json",
            "--artifacts",
            "originals",
        ]);
    }

    fn vendored(&self) -> Vec<PathBuf> {
        let vendor = self.root().join(".isolock/vendor/cargo");
        let entry = fs::read_dir(&vendor).unwrap().next().unwrap().unwrap();
        fs::read_dir(entry.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect()
    }
}

#[test]
fn a_planned_git_dependency_materialises_into_a_vendored_tree() {
    let world = World::new();
    world.configure();
    world.ingest();
    world.success(&["plan", "--profile", PROFILE, "--platform", PLATFORM]);
    world.success(&[
        "sync",
        "--profile",
        PROFILE,
        "--platform",
        PLATFORM,
        "--locked",
        "--offline",
    ]);

    // The vendor directory also holds the registry crates Cargo needs; the git
    // dependency is the commit-addressed one.
    let packages = world.vendored();
    let packages = packages
        .iter()
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("gitdep")
        })
        .collect::<Vec<_>>();
    assert_eq!(packages.len(), 1, "one vendored git package");
    let package = packages[0];
    // The directory is commit-addressed, so two commits never share a tree.
    assert!(
        package
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("gitdep-"),
        "{package:?}"
    );
    assert!(package.join("Cargo.toml").is_file());
    assert!(package.join("src/lib.rs").is_file());

    // Cargo requires this file and fails without it; the digest it lists is real.
    let checksum: serde_json::Value =
        serde_json::from_slice(&fs::read(package.join(".cargo-checksum.json")).unwrap()).unwrap();
    assert_eq!(checksum["package"], serde_json::Value::Null);
    let files = checksum["files"].as_object().unwrap();
    assert_eq!(
        files["Cargo.toml"],
        json!(digest_file(&package.join("Cargo.toml")).unwrap())
    );
    assert_eq!(
        files["src/lib.rs"],
        json!(digest_file(&package.join("src/lib.rs")).unwrap())
    );

    // The generated Cargo config replaces the git source, not a registry.
    assert_generated_config(&world);
}

/// Asserts the generated Cargo config replaces the git source properly.
fn assert_generated_config(world: &World) {
    // Locate the generated Cargo home rather than guessing its depth.
    let profiles = world.root().join(".isolock/profiles");
    let mut cargo_home = None;
    let mut pending = vec![profiles];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.ends_with("cargo-home") {
                    cargo_home = Some(path);
                } else {
                    pending.push(path);
                }
            }
        }
    }
    let cargo_home = cargo_home.expect("generated Cargo home");
    let config = fs::read_to_string(cargo_home.join("config.toml")).unwrap();
    assert!(
        config.contains("[source.\"git+file://"),
        "expected a git source stanza:\n{config}"
    );
    assert!(config.contains("rev = "), "{config}");
    assert!(!config.contains("registry = \"git+"), "{config}");
}

#[test]
fn the_vendored_tree_is_detected_when_modified() {
    let world = World::new();
    world.configure();
    world.ingest();
    world.success(&["plan", "--profile", PROFILE, "--platform", PLATFORM]);
    world.success(&[
        "sync",
        "--profile",
        PROFILE,
        "--platform",
        PLATFORM,
        "--locked",
        "--offline",
    ]);

    let packages = world.vendored();
    let package = packages.into_iter().next().unwrap();
    fs::write(
        package.join("src/lib.rs"),
        "pub fn answer() -> u32 { 99 }\n",
    )
    .unwrap();

    // Restoration must refuse to reuse a tree that no longer matches its receipt.
    let output = world.run(&[
        "sync",
        "--profile",
        PROFILE,
        "--platform",
        PLATFORM,
        "--locked",
        "--offline",
    ]);
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("modified") || stderr.contains("different derivation"),
        "expected a tampered-tree rejection, got {stderr}"
    );
}

#[test]
fn the_artifact_is_described_as_a_git_dependency() {
    let world = World::new();
    world.configure();
    world.ingest();
    world.success(&["plan", "--profile", PROFILE, "--platform", PLATFORM]);

    let plans = world
        .root()
        .join(format!(".isolock/plans/{PROFILE}-{PLATFORM}.lock"));
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(&plans).unwrap()).unwrap();
    let artifact = manifest["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|artifact| artifact["kind"] == json!("cargo"))
        .unwrap();
    assert_eq!(artifact["metadata"]["origin"], "git");
    assert_eq!(artifact["metadata"]["revision"], world.commit.as_str());
    assert_eq!(
        artifact["metadata"]["name"], "gitdep",
        "materialisation needs the crate name without re-reading the lockfile"
    );

    // The derivation identity covers the pin, so two commits cannot share a tree.
    let derivation = manifest["cargo-payloads"].as_object().unwrap();
    let value = derivation.values().next().unwrap();
    assert_eq!(value["git"][0]["commit"], world.commit.as_str());
    let _ = ArtifactKind::Cargo;
}
