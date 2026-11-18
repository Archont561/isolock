#![cfg(unix)]

mod support;

use isolock_cas::{FileStore, Sha256Digest};
use isolock_plan::profile::{ArchiveFormat, PlatformTools};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::{archive, text, Fixture, PLATFORM, RUST_PROFILE};

#[test]
fn git_snapshot_fallback_is_reported_and_uses_the_locked_payload_commit() {
    let project = Fixture::new();
    project.plan(RUST_PROFILE);
    let repository = project.temp.path().join("storage");
    project.success(&[
        "export",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--output",
        repository.to_str().unwrap(),
    ]);
    git(&repository, &["init", "--quiet"]);
    commit(&repository);
    let source = fs::read_to_string(project.root.join("isolock.toml")).unwrap();
    fs::write(
        project.root.join("isolock.toml"),
        source.replace(
            "[storage]",
            &format!(
                "[storage]\ntransport='git'\nreference='HEAD'\nsource={}\n",
                toml::Value::String(repository.to_str().unwrap().into())
            ),
        ),
    )
    .unwrap();
    let plan = project.plan(RUST_PROFILE);
    assert_eq!(plan.storage.kind, "git");
    let payload_commit = plan.storage.commit.clone().unwrap();
    project.success(&[
        "export",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--output",
        repository.to_str().unwrap(),
    ]);
    commit(&repository);
    let head = git(&repository, &["rev-parse", "HEAD"]);
    assert_ne!(head.trim(), payload_commit);
    fs::remove_dir_all(project.cas().root()).unwrap();
    let output = project.success(&[
        "fetch",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--manifest",
        plan.digest().unwrap().as_str(),
    ]);
    assert!(text(&output).contains("broader storage snapshot"));
    assert_eq!(project.cas().digests().unwrap(), plan.reachable_blobs());
    let output = project
        .command(&[
            "plan",
            "--profile",
            RUST_PROFILE,
            "--platform",
            PLATFORM,
            "--ci",
        ])
        .env("ISOLOCK_STORAGE_REF", "other")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(text(&output).contains("storage.reference"));
}

fn git(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    String::from_utf8(output.stdout).unwrap()
}
fn commit(repository: &Path) {
    git(repository, &["add", "."]);
    git(
        repository,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture snapshot",
        ],
    );
}

#[test]
fn platform_specific_seeds_and_cargo_policy_resolve_without_importing_other_targets() {
    let mut project = Fixture::new();
    let mut seed = project.profiles["rust-standalone"].seeds["isolock"].clone();
    seed.platform = "osx-arm64".into();
    seed.digest = project.blob("mac-seed", b"mac seed bytes");
    let mut runtime = project.profiles["rust-standalone"].runtimes["rust"].clone();
    runtime.platform = "osx-arm64".into();
    let profile = project.profiles.get_mut("rust-standalone").unwrap();
    profile.platforms.push("osx-arm64".into());
    let mut cargo = profile.cargo.clone();
    cargo.target = "aarch64-apple-darwin".into();
    profile.platform_tools.insert(
        "osx-arm64".into(),
        PlatformTools {
            seeds: BTreeMap::from([("isolock".into(), seed.clone())]),
            runtimes: BTreeMap::from([("rust".into(), runtime)]),
            cargo: Some(cargo),
        },
    );
    project.write_config(None);
    let linux = project.plan("rust-standalone");
    project.success(&[
        "plan",
        "--profile",
        "rust-standalone",
        "--platform",
        "osx-arm64",
    ]);
    let path = project
        .root
        .join(".isolock/plans/rust-standalone-osx-arm64.lock");
    let mac: isolock_plan::profile::ProfileManifest =
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(mac.specification.seeds["isolock"], seed);
    assert!(mac
        .artifacts
        .iter()
        .all(|artifact| artifact.platform == "osx-arm64"));
    assert!(linux
        .artifacts
        .iter()
        .all(|artifact| artifact.platform == PLATFORM));
    assert_ne!(linux.cargo_payloads, mac.cargo_payloads);
    let output = project.run(&[
        "sync",
        "--profile",
        "rust-standalone",
        "--platform",
        "osx-arm64",
        "--offline",
    ]);
    assert!(!output.status.success());
    assert!(text(&output).contains("host fallback is forbidden"));
}

#[test]
fn archive_links_and_traversal_never_escape_private_staging() {
    for link in [false, true] {
        let mut project = Fixture::new();
        let content = project.temp.path().join("hostile");
        fs::create_dir(&content).unwrap();
        if link {
            std::os::unix::fs::symlink(project.temp.path().join("escaped"), content.join("escape"))
                .unwrap();
        } else {
            fs::write(content.join("escape"), b"must not escape").unwrap();
        }
        let path = project.temp.path().join("hostile.tar");
        let mut tar = Command::new("tar");
        tar.arg("-cf").arg(&path).arg("-C").arg(&content);
        if !link {
            tar.arg("--transform=s,^escape$,../escaped,");
        }
        tar.arg("escape");
        assert!(tar.status().unwrap().success());
        let bytes = fs::read(path).unwrap();
        let tool = project.tool("hostile-runtime", "runtime.tar", &bytes, ArchiveFormat::Tar);
        project
            .profiles
            .get_mut("rust-standalone")
            .unwrap()
            .runtimes
            .insert("rust".into(), tool);
        project.write_config(None);
        project.plan("rust-standalone");
        let output = project.run(&[
            "sync",
            "--profile",
            "rust-standalone",
            "--platform",
            PLATFORM,
            "--offline",
        ]);
        assert!(!output.status.success(), "{}", text(&output));
        assert!(!project.temp.path().join("escaped").exists());
        assert!(!project
            .root
            .join(".isolock/receipts/rust-standalone-linux-64.json")
            .exists());
    }
}

#[test]
fn locked_drift_preflights_invalid_inputs_and_environment_changes_before_transfer() {
    let project = Fixture::new();
    project.plan(RUST_PROFILE);
    let output = project
        .command(&[
            "sync",
            "--profile",
            RUST_PROFILE,
            "--platform",
            PLATFORM,
            "--locked",
        ])
        .env("ISOLOCK_PIXI_ENVIRONMENT", "not-present")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(text(&output).contains("specification.environment"));
    fs::write(project.root.join("pixi.lock"), "invalid lockfile").unwrap();
    let output = project.run(&[
        "plan",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--ci",
    ]);
    assert!(!output.status.success());
    assert!(text(&output).contains("inputs.pixi.lock"));
    assert!(!project.root.join(".isolock/transport").exists());
    assert!(!project.root.join(".isolock/profiles").exists());
}

#[test]
fn corrupt_channel_metadata_is_not_reused() {
    let project = Fixture::new();
    let manifest = project.plan(RUST_PROFILE);
    project.sync(RUST_PROFILE);
    let channel = project
        .root
        .join(".isolock/channels/conda")
        .join(manifest.digest().unwrap().as_str());
    let subchannel = fs::read_dir(&channel)
        .unwrap()
        .find_map(|entry| {
            let entry = entry.unwrap();
            entry.file_type().unwrap().is_dir().then(|| entry.path())
        })
        .unwrap();
    fs::write(
        subchannel.join("linux-64/repodata.json"),
        b"corrupt metadata",
    )
    .unwrap();
    let output = project.run(&[
        "sync",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--offline",
    ]);
    assert!(!output.status.success());
    assert!(text(&output).contains("modified"));
}

#[test]
fn gc_requires_complete_roots_and_refuses_noncanonical_or_symlinked_cas_paths() {
    let project = Fixture::new();
    project.plan(RUST_PROFILE);
    let garbage = Sha256Digest::from_bytes(b"garbage");
    project.cas().put_bytes(&garbage, b"garbage").unwrap();
    let output = project.run(&["gc", "--apply", "--root", "missing-linux-64"]);
    assert!(!output.status.success());
    assert!(project.cas().path(&garbage).is_file());
    let outside = project.temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let store = FileStore::new(project.temp.path().join("unsafe-cas"));
    fs::create_dir_all(store.root()).unwrap();
    std::os::unix::fs::symlink(&outside, store.root().join("sha256")).unwrap();
    assert!(store.put_bytes(&garbage, b"garbage").is_err());
    assert!(store.digests().is_err());
    assert!(fs::read_dir(outside).unwrap().next().is_none());
}

#[test]
fn offline_sync_never_uses_transport_even_with_an_explicit_manifest() {
    let project = Fixture::new();
    let manifest = project.plan(RUST_PROFILE);
    let seed = &manifest.specification.seeds["pixi"].digest;
    fs::remove_file(project.cas().path(seed)).unwrap();
    let output = project.run(&[
        "sync",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--manifest",
        manifest.digest().unwrap().as_str(),
        "--offline",
    ]);
    assert!(!output.status.success());
    assert!(text(&output).contains("missing artifacts"));
    assert!(!project.root.join(".isolock/transport").exists());
}

#[test]
fn javascript_sri_and_wheel_catalog_hashes_are_enforced() {
    let mut project = Fixture::new();
    let bytes = archive(
        &[(
            "package/package.json",
            "{\"name\":\"hello-cli\",\"version\":\"1.0.0\",\"bin\":{}}",
            false,
        )],
        "-czf",
    );
    let digest = project.blob("changed-js", &bytes);
    project
        .catalog
        .iter_mut()
        .find(|entry| entry.name == "hello-cli")
        .unwrap()
        .digest = digest;
    project.write_catalog();
    project.plan("bun-standalone");
    let output = project.run(&[
        "sync",
        "--profile",
        "bun-standalone",
        "--platform",
        PLATFORM,
        "--offline",
    ]);
    assert!(!output.status.success());
    assert!(text(&output).contains("integrity mismatch"));
    project
        .catalog
        .iter_mut()
        .find(|entry| entry.name == "demo")
        .unwrap()
        .digest = Sha256Digest::from_bytes(b"different");
    project.write_catalog();
    let output = project.run(&[
        "plan",
        "--profile",
        "python-standalone",
        "--platform",
        PLATFORM,
    ]);
    assert!(!output.status.success());
    assert!(text(&output).contains("catalog wheel"));
}

#[test]
fn pixi_must_link_exactly_the_expected_conda_record_set() {
    let mut project = Fixture::new();
    let seed = String::from_utf8(project.blobs["pixi-seed"].clone()).unwrap();
    let conda = Sha256Digest::from_bytes(&project.blobs["conda-rust"]);
    let seed = seed.replace(conda.as_str(), &"0".repeat(64));
    let tool = project.tool(
        "wrong-pixi-output",
        "pixi",
        seed.as_bytes(),
        ArchiveFormat::Raw,
    );
    project
        .profiles
        .get_mut(RUST_PROFILE)
        .unwrap()
        .seeds
        .insert("pixi".into(), tool);
    project.write_config(None);
    project.plan(RUST_PROFILE);
    let output = project.run(&[
        "sync",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--offline",
    ]);
    assert!(!output.status.success());
    assert!(text(&output).contains("installed Conda prefix does not match"));
    assert!(!project
        .root
        .join(".isolock/receipts/rust-pixi-linux-64.json")
        .exists());
    assert!(!project.root.join(".pixi/envs/rust").exists());
}

#[test]
fn local_manifest_selection_needs_no_remote_source() {
    let project = Fixture::new();
    let manifest = project.plan(RUST_PROFILE);
    project.success(&[
        "sync",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--manifest",
        manifest.digest().unwrap().as_str(),
        "--locked",
    ]);
    assert!(project.bin(RUST_PROFILE, "cargo").is_file());
}
