#![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures fail fast on invalid setup.
//! Opt-in integration with tools restored from the reviewed developer sandbox.
//! CI supplies explicit profile/platform and tool paths; these tests never download.
#![cfg(target_os = "linux")]

mod support;

use isolock_cas::{digest_file, FileStore, Sha256Digest};
use isolock_plan::profile::{ArchiveFormat, ProfileManifest, ToolArtifact};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use support::{archive, text, Fixture, PLATFORM, RUST_PROFILE};

#[test]
#[ignore = "requires ISOLOCK_TEST_PIXI from the restored developer sandbox"]
#[allow(clippy::too_many_lines)] // End-to-end fixture setup and lifecycle assertions stay together.
fn real_pixi_restores_the_selected_profile_from_a_cold_offline_cache() {
    assert_eq!(std::env::var("ISOLOCK_TEST_PROFILE").unwrap(), RUST_PROFILE);
    assert_eq!(std::env::var("ISOLOCK_TEST_PLATFORM").unwrap(), PLATFORM);
    let pixi = PathBuf::from(std::env::var_os("ISOLOCK_TEST_PIXI").unwrap())
        .canonicalize()
        .unwrap();
    let project = real_pixi_project(&pixi);
    project.plan(RUST_PROFILE);
    project.success(&[
        "seed-manifest",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
    ]);
    let output = Command::new("/bin/sh")
        .arg(project.root.join(".isolock/bootstrap.sh"))
        .args([
            "--profile",
            RUST_PROFILE,
            "--platform",
            PLATFORM,
            "--offline",
        ])
        .env("PATH", "/usr/bin:/bin")
        .current_dir(&project.root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let output = Command::new(project.bin(RUST_PROFILE, "cargo"))
        .arg("build")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("real-pixi-linked-rust"));
    assert!(project.root.join(".pixi/envs/rust/bin/cargo").is_file());
    assert!(!project.root.join(".pixi/envs/rust/bin/python").exists());
    assert!(!project.root.join(".pixi/envs/rust/bin/bun").exists());
    let receipt = project.receipt(RUST_PROFILE);
    assert_eq!(receipt["environment"], "rust");
    assert_eq!(receipt["derived-inputs"].as_object().unwrap().len(), 4);
    // Reuse also verifies the channel seal and exact installed records.
    project.sync(RUST_PROFILE);
}

#[test]
#[ignore = "requires ISOLOCK_TEST_PIXI from the restored developer sandbox"]
fn real_pixi_conda_profile_roundtrips_through_git_storage_and_installs_offline() {
    assert_eq!(std::env::var("ISOLOCK_TEST_PROFILE").unwrap(), RUST_PROFILE);
    assert_eq!(std::env::var("ISOLOCK_TEST_PLATFORM").unwrap(), PLATFORM);
    let pixi = PathBuf::from(std::env::var_os("ISOLOCK_TEST_PIXI").unwrap())
        .canonicalize()
        .unwrap();
    let producer = real_pixi_project(&pixi);
    let remote = producer.temp.path().join("conda-storage.git");
    let (originals, profile) = publish_conda_profile(&producer, &remote);
    let digest = profile.digest().unwrap().as_str().to_owned();
    restore_conda_consumer(&producer, &remote, &originals, &profile, &digest);
}

fn publish_conda_profile(producer: &Fixture, remote: &Path) -> (PathBuf, ProfileManifest) {
    assert!(Command::new("git")
        .args(["init", "--bare", "--quiet"])
        .arg(remote)
        .status()
        .unwrap()
        .success());
    git(
        &producer.root,
        &["init", "--quiet", "--initial-branch=main"],
    );
    git(
        &producer.root,
        &["config", "user.name", "Conda fixture publisher"],
    );
    git(
        &producer.root,
        &["config", "user.email", "conda-fixture@example.invalid"],
    );
    git(
        &producer.root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    configure_git_storage(producer, remote);

    let originals = write_reviewed_conda_inputs(producer);
    let cas_root = producer.cas().root().to_path_buf();
    fs::remove_dir_all(&cas_root).unwrap();
    producer.success(&[
        "ingest",
        "--manifest",
        "isolock.ingest.json",
        "--artifacts",
        originals.to_str().unwrap(),
    ]);
    producer.success(&["push", "--manifest", "isolock.ingest.json"]);

    let profile = producer.plan(RUST_PROFILE);
    let producer_digests = producer.cas().digests().unwrap();
    let missing = profile
        .reachable_blobs()
        .difference(&producer_digests)
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "producer CAS is missing {missing:?}; profile inputs are {:?}; profile artifacts are {:?}",
        profile.inputs,
        profile.artifacts
    );
    producer.success(&["push", "--profile", RUST_PROFILE, "--platform", PLATFORM]);
    assert_eq!(
        String::from_utf8(git(remote, &["rev-parse", "refs/heads/isolock/v1"]))
            .unwrap()
            .trim()
            .len(),
        40,
        "the profile and payloads are published on the storage branch"
    );
    (originals, profile)
}

fn restore_conda_consumer(
    producer: &Fixture,
    remote: &Path,
    originals: &Path,
    profile: &ProfileManifest,
    digest: &str,
) {
    // A separate project starts without producer CAS data or a warmed Pixi prefix.
    let mut consumer = Fixture::new();
    consumer.profiles = producer.profiles.clone();
    for input in ["Cargo.lock", "Cargo.toml", "pixi.lock", "pixi.toml"] {
        fs::copy(producer.root.join(input), consumer.root.join(input)).unwrap();
    }
    configure_git_storage(&consumer, remote);
    let consumer_cas = consumer.cas().root().to_path_buf();
    fs::remove_dir_all(&consumer_cas).unwrap();
    assert!(
        !consumer_cas.exists(),
        "consumer starts with a cold Isolock CAS"
    );

    consumer.success(&[
        "fetch",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--manifest",
        digest,
    ]);
    assert_eq!(
        FileStore::new(&consumer_cas).digests().unwrap(),
        profile.reachable_blobs(),
        "fetch restores exactly the reviewed profile closure"
    );

    // Prove neither producer bytes nor the remote are needed once the closure is fetched.
    fs::remove_dir_all(remote).unwrap();
    fs::remove_dir_all(originals).unwrap();
    fs::remove_dir_all(producer.cas().root()).unwrap();
    consumer.sync(RUST_PROFILE);
    assert_installed_conda_fixture(&consumer);

    // A second offline sync exercises cache reuse and checks the installed record set again.
    consumer.sync(RUST_PROFILE);
}

fn assert_installed_conda_fixture(consumer: &Fixture) {
    let prefix = consumer.root.join(".pixi/envs/rust");
    assert!(prefix.join("bin/cargo").is_file());
    let installed_records = fs::read_dir(prefix.join("conda-meta"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .map(|path| serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        installed_records.len(),
        1,
        "only the locked package is installed"
    );
    assert_eq!(installed_records[0]["name"], "rust");
    assert_eq!(installed_records[0]["version"], "1.0.0");
    assert_eq!(installed_records[0]["build"], "0");

    let output = Command::new(consumer.bin(RUST_PROFILE, "cargo"))
        .arg("build")
        .current_dir(&consumer.root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("real-pixi-linked-rust"));
}

fn real_pixi_project(pixi_path: &Path) -> Fixture {
    let mut project = Fixture::new();
    let old = Sha256Digest::from_bytes(&project.blobs["conda-rust"]);
    let record = r#"{"name":"rust","version":"1.0.0","build":"0","build_number":0,"subdir":"linux-64","depends":[],"license":"MIT"}"#;
    let cargo = "#!/bin/sh\n[ \"${CARGO_NET_OFFLINE:-}\" = true ] || exit 71\n[ -f \"$CARGO_HOME/config.toml\" ] || exit 72\necho real-pixi-linked-rust\n";
    let bytes = archive(
        &[
            ("info/index.json", record, false),
            ("info/files", "bin/cargo\nbin/rustc\n", false),
            ("bin/cargo", cargo, true),
            ("bin/rustc", "#!/bin/sh\necho rustc-fixture\n", true),
        ],
        "-cjf",
    );
    let digest = project.blob("real-conda-rust", &bytes);
    let lock = fs::read_to_string(project.root.join("pixi.lock"))
        .unwrap()
        .replace(old.as_str(), digest.as_str())
        .replacen(
            &format!("  size: {}\n", project.blobs["conda-rust"].len()),
            &format!("  size: {}\n", bytes.len()),
            1,
        );
    fs::write(project.root.join("pixi.lock"), lock).unwrap();
    let isolock_path = project.temp.path().join("isolock-seed");
    fs::copy(env!("CARGO_BIN_EXE_isolock"), &isolock_path).unwrap();
    assert!(Command::new("strip")
        .args(["--strip-unneeded"])
        .arg(&isolock_path)
        .status()
        .unwrap()
        .success());
    assert!(fs::metadata(&isolock_path).unwrap().len() < 95 * 1024 * 1024);
    project.blob("real-pixi-seed", &fs::read(pixi_path).unwrap());
    project.blob("real-isolock-seed", &fs::read(&isolock_path).unwrap());
    let pixi = external_tool(&project, pixi_path, "pixi");
    let isolock = external_tool(&project, &isolock_path, "isolock");
    let profile = project.profiles.get_mut(RUST_PROFILE).unwrap();
    profile.seeds.insert("pixi".into(), pixi);
    profile.seeds.insert("isolock".into(), isolock);
    project.write_config(None);
    project
}

fn write_reviewed_conda_inputs(project: &Fixture) -> PathBuf {
    let originals = project.root.join("originals");
    fs::create_dir_all(&originals).unwrap();
    let mut artifacts = Vec::new();

    for (key, bytes) in &project.blobs {
        if key == "real-conda-rust" {
            let filename = "rust-1.0.0-0.tar.bz2";
            fs::write(originals.join(filename), bytes).unwrap();
            artifacts.push(json!({
                "registry": "conda",
                "name": "rust",
                "version": "1.0.0",
                "filename": filename,
                "source": "https://packages.example.invalid/linux-64/rust-1.0.0-0.tar.bz2",
                "sha256": Sha256Digest::from_bytes(bytes).as_str(),
                "metadata": {
                    "subdir": "linux-64",
                    "registry_metadata": {
                        "name": "rust",
                        "version": "1.0.0",
                        "build": "0",
                        "build_number": 0,
                        "subdir": "linux-64",
                        "depends": []
                    }
                }
            }));
        } else {
            let filename = format!("{key}.bin");
            fs::write(originals.join(&filename), bytes).unwrap();
            artifacts.push(json!({
                "registry": "generic",
                "name": format!("fixture-{key}"),
                "version": "1.0.0",
                "filename": filename,
                "source": format!("https://fixtures.example.invalid/{filename}"),
                "sha256": Sha256Digest::from_bytes(bytes).as_str()
            }));
        }
    }

    for (name, filename) in [
        ("cargo-lock", "Cargo.lock"),
        ("cargo-manifest", "Cargo.toml"),
        ("pixi-lock", "pixi.lock"),
        ("pixi-manifest", "pixi.toml"),
    ] {
        let bytes = fs::read(project.root.join(filename)).unwrap();
        fs::write(originals.join(filename), &bytes).unwrap();
        artifacts.push(json!({
            "registry": "generic",
            "name": name,
            "version": "1.0.0",
            "filename": filename,
            "source": format!("https://fixtures.example.invalid/{filename}"),
            "sha256": Sha256Digest::from_bytes(&bytes).as_str()
        }));
    }

    fs::write(
        project.root.join("isolock.ingest.json"),
        serde_json::to_vec_pretty(&json!({ "schema-version": 1, "artifacts": artifacts })).unwrap(),
    )
    .unwrap();
    originals
}

fn configure_git_storage(project: &Fixture, remote: &Path) {
    project.write_config(Some(remote));
    let path = project.root.join("isolock.toml");
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["storage"]
        .as_table_mut()
        .unwrap()
        .insert("transport".into(), toml::Value::String("git".into()));
    fs::write(path, toml::to_string(&config).unwrap()).unwrap();
}

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    output.stdout
}

#[test]
#[ignore = "requires ISOLOCK_TEST_RUST_PREFIX from the restored developer sandbox"]
fn real_standalone_rust_builds_a_locked_registry_dependency_offline() {
    assert_eq!(
        std::env::var("ISOLOCK_TEST_PROFILE").unwrap(),
        "rust-standalone"
    );
    assert_eq!(std::env::var("ISOLOCK_TEST_PLATFORM").unwrap(), PLATFORM);
    let source = PathBuf::from(std::env::var_os("ISOLOCK_TEST_RUST_PREFIX").unwrap())
        .canonicalize()
        .unwrap();
    let mut project = Fixture::new();
    let archive = project.temp.path().join("rust-prefix.tar");
    let mut tar = Command::new("tar");
    tar.args(["--dereference", "--hard-dereference", "-cf"])
        .arg(&archive)
        .arg("-C")
        .arg(&source)
        .args(["bin/cargo", "bin/rustc", "lib/rustlib"]);
    for entry in fs::read_dir(source.join("lib")).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        if name.starts_with("librustc_driver")
            || name.starts_with("libLLVM")
            || name.starts_with("libgcc_s.so")
            || name.starts_with("libz.so")
        {
            tar.arg(format!("lib/{name}"));
        }
    }
    assert!(tar.status().unwrap().success());
    let mut rust = external_tool(&project, &archive, "rust-prefix.tar");
    rust.format = ArchiveFormat::Tar;
    let isolock = external_tool(
        &project,
        Path::new(env!("CARGO_BIN_EXE_isolock")),
        "isolock",
    );
    let profile = project.profiles.get_mut("rust-standalone").unwrap();
    profile.runtimes.insert("rust".into(), rust);
    profile.seeds.insert("isolock".into(), isolock);
    let version = Command::new(source.join("bin/cargo"))
        .arg("--version")
        .output()
        .unwrap();
    profile.cargo.cargo_version = String::from_utf8(version.stdout)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .into();
    project.write_config(None);
    project.plan("rust-standalone");
    project.sync("rust-standalone");
    let output = Command::new(project.bin("rust-standalone", "cargo"))
        .args(["build", "--locked", "--offline"])
        .current_dir(&project.root)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env(
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
            "/usr/bin/cc",
        )
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let output = Command::new(project.root.join("target/debug/app"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(text(&output).trim(), "42");
}

fn external_tool(project: &Fixture, path: &Path, filename: &str) -> ToolArtifact {
    let digest = digest_file(path).unwrap();
    project.cas().put_file(&digest, path).unwrap();
    ToolArtifact {
        version: "1.0.0".into(),
        platform: PLATFORM.into(),
        digest,
        filename: filename.into(),
        source: format!("https://github.com/fixture/reviewed/{filename}"),
        format: ArchiveFormat::Raw,
        entry: None,
    }
}

/// A real offline build of a Cargo git dependency, with its origin deleted.
///
/// This is the claim the whole feature exists for: after restoration, the
/// vendored commit tree must satisfy `cargo build --locked --offline` even when
/// the repository it came from no longer exists anywhere on disk.
#[test]
#[ignore = "requires ISOLOCK_TEST_RUST_PREFIX from the restored developer sandbox"]
fn real_standalone_rust_builds_a_locked_git_dependency_offline() {
    let (project, upstream) = git_dependency_project();
    project.plan("rust-standalone");
    project.sync("rust-standalone");

    // The origin must be unreachable, or this proves nothing.
    fs::remove_dir_all(&upstream).unwrap();
    assert!(!upstream.exists());

    let output = Command::new(project.bin("rust-standalone", "cargo"))
        .args(["build", "--locked", "--offline"])
        .current_dir(&project.root)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env(
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
            "/usr/bin/cc",
        )
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let output = Command::new(project.root.join("target/debug/app"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(text(&output).trim(), "42");
}

/// Builds a restored profile with one real git dependency already ingested.
#[allow(clippy::too_many_lines)] // A reviewed end-to-end fixture reads as one sequence.
fn git_dependency_project() -> (Fixture, PathBuf) {
    assert_eq!(
        std::env::var("ISOLOCK_TEST_PROFILE").unwrap(),
        "rust-standalone"
    );
    assert_eq!(std::env::var("ISOLOCK_TEST_PLATFORM").unwrap(), PLATFORM);
    let source = PathBuf::from(std::env::var_os("ISOLOCK_TEST_RUST_PREFIX").unwrap())
        .canonicalize()
        .unwrap();
    let mut project = Fixture::new();
    let rust = rust_prefix(&project, &source);
    let isolock = external_tool(
        &project,
        Path::new(env!("CARGO_BIN_EXE_isolock")),
        "isolock",
    );
    let profile = project.profiles.get_mut("rust-standalone").unwrap();
    profile.runtimes.insert("rust".into(), rust);
    profile.seeds.insert("isolock".into(), isolock);
    profile.cargo.cargo_version = cargo_version(&source);
    profile.cargo_git.clear();

    // A real repository holding one crate, pinned to one commit.
    let upstream = project.temp.path().join("upstream");
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
    // The sandbox has no configured identity, and a commit needs one.
    git(
        &upstream,
        &[
            "-c",
            "user.email=reviewed@example.test",
            "-c",
            "user.name=reviewed",
            "commit",
            "--quiet",
            "-m",
            "one",
        ],
    );
    let commit = String::from_utf8(git(&upstream, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_owned();
    let repository = format!("file://{}", upstream.to_str().unwrap());

    // The lockfile and manifest name that repository and commit.
    fs::write(
        project.root.join("Cargo.lock"),
        format!(
            "version = 4\n\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\"gitdep\"]\n\n[[package]]\nname = \"gitdep\"\nversion = \"0.1.0\"\nsource = \"git+{repository}?rev={commit}#{commit}\"\n"
        ),
    )
    .unwrap();
    fs::write(
        project.root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ngitdep = {{ git = \"{repository}\", rev = \"{commit}\" }}\n"
        ),
    )
    .unwrap();
    fs::write(
        project.root.join("src/main.rs"),
        "fn main() { println!(\"{}\", gitdep::answer()); }\n",
    )
    .unwrap();

    // The reviewed pin is the digest of `git archive` for that commit.
    let archive = project.temp.path().join("exported.tar");
    assert!(Command::new("git")
        .args([
            "-C",
            upstream.to_str().unwrap(),
            "archive",
            "--format=tar",
            "--output",
            archive.to_str().unwrap(),
            &commit,
        ])
        .status()
        .unwrap()
        .success());
    let digest = digest_file(&archive).unwrap();
    project
        .profiles
        .get_mut("rust-standalone")
        .unwrap()
        .cargo_git = vec![isolock_plan::profile::CargoGitPin {
        name: "gitdep".into(),
        version: "0.1.0".into(),
        repository: repository.clone(),
        commit: commit.clone(),
        digest: digest.clone(),
    }];

    // Ingest the pinned commit, exactly as a reviewed workflow would.
    fs::write(
        project.root.join("isolock.ingest.json"),
        serde_json::to_vec_pretty(&json!({
            "schema-version": 1,
            "artifacts": [{
                "registry": "cargo",
                "name": "gitdep",
                "version": "0.1.0",
                "sha256": digest,
                "filename": "gitdep-git.tar",
                "source": repository,
                "git": {
                    "repository": repository,
                    "revision": commit,
                    "commit": commit
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::create_dir_all(project.root.join("originals")).unwrap();
    project.write_config(None);
    project.success(&[
        "ingest",
        "--manifest",
        "isolock.ingest.json",
        "--artifacts",
        "originals",
    ]);

    (project, upstream)
}

/// Packs the restored Rust prefix into a reviewed runtime artifact.
fn rust_prefix(project: &Fixture, source: &Path) -> ToolArtifact {
    let archive = project.temp.path().join("rust-prefix.tar");
    let mut tar = Command::new("tar");
    tar.args(["--dereference", "--hard-dereference", "-cf"])
        .arg(&archive)
        .arg("-C")
        .arg(source)
        .args(["bin/cargo", "bin/rustc", "lib/rustlib"]);
    for entry in fs::read_dir(source.join("lib")).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        if name.starts_with("librustc_driver")
            || name.starts_with("libLLVM")
            || name.starts_with("libgcc_s.so")
            || name.starts_with("libz.so")
        {
            tar.arg(format!("lib/{name}"));
        }
    }
    assert!(tar.status().unwrap().success());
    let mut rust = external_tool(project, &archive, "rust-prefix.tar");
    rust.format = ArchiveFormat::Tar;
    rust
}

/// The exact Cargo version the profile must record, to match the restored toolchain.
fn cargo_version(source: &Path) -> String {
    let version = Command::new(source.join("bin/cargo"))
        .arg("--version")
        .output()
        .unwrap();
    String::from_utf8(version.stdout)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .into()
}

/// A materialized standalone Rust profile whose lock names `hello` but whose manifest
/// does not depend on it, so `cargo add` is the only thing that can introduce it.
fn locked_hello_project() -> Fixture {
    assert_eq!(
        std::env::var("ISOLOCK_TEST_PROFILE").unwrap(),
        "rust-standalone"
    );
    assert_eq!(std::env::var("ISOLOCK_TEST_PLATFORM").unwrap(), PLATFORM);
    let source = PathBuf::from(std::env::var_os("ISOLOCK_TEST_RUST_PREFIX").unwrap())
        .canonicalize()
        .unwrap();
    let mut project = Fixture::new();
    let rust = rust_prefix(&project, &source);
    let isolock = external_tool(
        &project,
        Path::new(env!("CARGO_BIN_EXE_isolock")),
        "isolock",
    );
    let profile = project.profiles.get_mut("rust-standalone").unwrap();
    profile.runtimes.insert("rust".into(), rust);
    profile.seeds.insert("isolock".into(), isolock);
    profile.cargo.cargo_version = cargo_version(&source);
    // `hello` is reviewed and locked, but is not yet a direct dependency. That is the
    // adoption workflow: the crate's bytes are sealed into the profile, and `cargo add`
    // promotes it to `[dependencies]` without resolving anything over the network.
    fs::write(
        project.root.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    // The checksum is the reviewed digest of the sealed crate archive, which is the
    // same anchor the proxy publisher uses.
    let checksum = Sha256Digest::from_bytes(project.blobs.get("cargo").unwrap());
    fs::write(
        project.root.join("Cargo.lock"),
        format!(
            "version = 4\n\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\n \"hello\",\n]\n\n[[package]]\nname = \"hello\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{}\"\n",
            checksum.as_str()
        ),
    )
    .unwrap();
    fs::write(project.root.join("src/main.rs"), "fn main() {}\n").unwrap();
    project.write_config(None);
    project.plan("rust-standalone");
    project.sync("rust-standalone");

    let vendor = project.root.join(".isolock/vendor/cargo");
    let sealed = fs::read_dir(&vendor)
        .unwrap()
        .flat_map(|key| fs::read_dir(key.unwrap().path()).unwrap())
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        sealed.iter().any(|name| name.starts_with("hello-1.0.0")),
        "the reviewed crate was not vendored: {sealed:?}"
    );

    project
}

/// A real `cargo add` adds a reviewed crate inside a profile session, with no network.
///
/// ISO-36 AC #2. The crate is sealed into the profile's vendor payload but is absent
/// from the locked graph, so only `cargo add` can introduce it. Nothing here is a host
/// tool and the source is a local directory, so a successful add proves the dependency
/// came from the reviewed snapshot rather than a host registry.
#[test]
#[ignore = "requires ISOLOCK_TEST_RUST_PREFIX from the restored developer sandbox"]
fn real_cargo_add_adds_a_reviewed_crate_offline() {
    let project = locked_hello_project();

    let output = Command::new(project.bin("rust-standalone", "cargo"))
        .args(["add", "hello"])
        .current_dir(&project.root)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env(
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
            "/usr/bin/cc",
        )
        .output()
        .unwrap();
    let report = text(&output);
    assert!(output.status.success(), "cargo add failed:\n{report}");
    let manifest = fs::read_to_string(project.root.join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("hello"),
        "cargo add did not record the dependency:\n{manifest}"
    );

    // AC #4: a crate absent from the reviewed snapshot is a miss, never an upstream
    // fetch. The source is local, so Cargo reports the index miss instead of dialling
    // out, and the manifest is left untouched.
    let miss = Command::new(project.bin("rust-standalone", "cargo"))
        .args(["add", "definitely-not-reviewed"])
        .current_dir(&project.root)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env(
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
            "/usr/bin/cc",
        )
        .output()
        .unwrap();
    let miss = text(&miss);
    assert!(
        miss.contains("could not be found in registry index"),
        "an unreviewed crate was not a clean miss:\n{miss}"
    );
    assert!(
        !miss.contains("spurious network error") && !miss.contains("failed to download"),
        "an unreviewed crate reached the network:\n{miss}"
    );
    let after = fs::read_to_string(project.root.join("Cargo.toml")).unwrap();
    assert!(
        !after.contains("definitely-not-reviewed"),
        "a miss still changed the manifest:\n{after}"
    );
}

/// A real `pixi add` resolves a reviewed package from the generated channel.
///
/// ISO-36 AC #3. `exec` exposes a Conda channel built from the profile manifest, so a
/// child `pixi add` must read that channel and record the package without contacting an
/// upstream channel. The manifest names a package that is already reviewed, because the
/// channel only ever contains reviewed bytes.
#[test]
#[ignore = "requires ISOLOCK_TEST_PIXI from the restored developer sandbox"]
fn real_pixi_add_resolves_from_the_generated_channel() {
    assert_eq!(std::env::var("ISOLOCK_TEST_PROFILE").unwrap(), RUST_PROFILE);
    assert_eq!(std::env::var("ISOLOCK_TEST_PLATFORM").unwrap(), PLATFORM);
    let pixi_path = PathBuf::from(std::env::var_os("ISOLOCK_TEST_PIXI").unwrap())
        .canonicalize()
        .unwrap();
    let project = real_pixi_project(&pixi_path);
    project.plan(RUST_PROFILE);
    project.sync(RUST_PROFILE);

    // A fresh workspace, so pixi is not asked to adopt the relocated environment that
    // `sync` installed. This is the real `add` journey: an empty manifest plus a resolve.
    let fresh = project.root.join("fresh");
    fs::create_dir_all(&fresh).unwrap();
    fs::write(
        fresh.join("pixi.toml"),
        "[workspace]\nname = \"fresh\"\nchannels = [\"https://conda.anaconda.org/conda-forge\"]\nplatforms = [\"linux-64\"]\n",
    )
    .unwrap();
    let output = project
        .command(&[
            "exec",
            "--profile",
            RUST_PROFILE,
            "--platform",
            PLATFORM,
            "--",
            "pixi",
            "add",
            "rust==1.0.0",
            "--manifest-path",
            "fresh/pixi.toml",
        ])
        .output()
        .unwrap();
    let report = text(&output);
    let manifest = fs::read_to_string(fresh.join("pixi.toml")).unwrap();
    assert!(
        output.status.success(),
        "pixi add did not resolve from the generated channel:\n{report}"
    );
    assert!(
        manifest.contains("rust"),
        "pixi add did not record the reviewed package:\n{manifest}"
    );
}

/// A `pixi add` for an unreviewed package fails without contacting an upstream channel.
///
/// ISO-36 AC #4 for the Conda projection.
#[test]
#[ignore = "requires ISOLOCK_TEST_PIXI from the restored developer sandbox"]
fn real_pixi_add_reports_a_miss_for_an_unreviewed_package() {
    assert_eq!(std::env::var("ISOLOCK_TEST_PROFILE").unwrap(), RUST_PROFILE);
    assert_eq!(std::env::var("ISOLOCK_TEST_PLATFORM").unwrap(), PLATFORM);
    let pixi_path = PathBuf::from(std::env::var_os("ISOLOCK_TEST_PIXI").unwrap())
        .canonicalize()
        .unwrap();
    let project = real_pixi_project(&pixi_path);
    project.plan(RUST_PROFILE);
    project.sync(RUST_PROFILE);

    let fresh = project.root.join("fresh");
    fs::create_dir_all(&fresh).unwrap();
    fs::write(
        fresh.join("pixi.toml"),
        "[workspace]\nname = \"fresh\"\nchannels = [\"https://conda.anaconda.org/conda-forge\"]\nplatforms = [\"linux-64\"]\n",
    )
    .unwrap();
    let output = project
        .command(&[
            "exec",
            "--profile",
            RUST_PROFILE,
            "--platform",
            PLATFORM,
            "--",
            "pixi",
            "add",
            "definitely-not-reviewed==9.9.9",
            "--manifest-path",
            "fresh/pixi.toml",
        ])
        .output()
        .unwrap();
    let report = text(&output);
    assert!(
        !output.status.success(),
        "an unreviewed Conda package was added:\n{report}"
    );
    let manifest = fs::read_to_string(fresh.join("pixi.toml")).unwrap();
    assert!(
        !manifest.contains("definitely-not-reviewed"),
        "a Conda miss still changed the manifest:\n{manifest}"
    );
}
