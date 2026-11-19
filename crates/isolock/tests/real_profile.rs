#![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures fail fast on invalid setup.
//! Opt-in integration with tools restored from the reviewed developer sandbox.
//! CI supplies explicit profile/platform and tool paths; these tests never download.
#![cfg(target_os = "linux")]

mod support;

use isolock_cas::{digest_file, Sha256Digest};
use isolock_plan::profile::{ArchiveFormat, ToolArtifact};
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
    let pixi = external_tool(&project, &pixi, "pixi");
    let isolock = external_tool(
        &project,
        Path::new(env!("CARGO_BIN_EXE_isolock")),
        "isolock",
    );
    let profile = project.profiles.get_mut(RUST_PROFILE).unwrap();
    profile.seeds.insert("pixi".into(), pixi);
    profile.seeds.insert("isolock".into(), isolock);
    project.write_config(None);
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
