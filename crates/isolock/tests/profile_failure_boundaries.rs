//! Profile publication, retention and materialization failures through the public CLI.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid fixtures must fail the test.

mod support;

use isolock_cas::Sha256Digest;
use isolock_plan::profile::{canonical_digest, ArchiveFormat};
use serde_json::{json, Value};
use std::fs;
use support::{archive, text, Fixture, PLATFORM, RUST_PROFILE};

fn failure(project: &Fixture, arguments: &[&str], expected: &str) {
    let output = project.run(arguments);
    assert!(!output.status.success(), "{}", text(&output));
    assert!(text(&output).contains(expected), "{}", text(&output));
}

#[test]
fn invalid_shared_gc_registrations_never_delete_unreachable_bytes() {
    let project = Fixture::new();
    project.plan(RUST_PROFILE);
    let garbage = Sha256Digest::from_bytes(b"unreachable");
    project.cas().put_bytes(&garbage, b"unreachable").unwrap();
    let directory = project.cas().root().join(".isolock-roots");
    let foreign = project.temp.path().join("other-workspace");
    let path = directory.join(format!(
        "{}.json",
        canonical_digest(&foreign).unwrap().as_str()
    ));
    for (value, expected) in [
        (json!({}), "invalid shared CAS roots"),
        (
            json!({"schema-version":2,"workspace":foreign,"roots":[],"blobs":[]}),
            "unsupported shared CAS root schema",
        ),
        (
            json!({"schema-version":1,"workspace":foreign,"roots":[],"blobs":["not-a-digest"]}),
            "SHA-256 digest",
        ),
    ] {
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        failure(&project, &["gc", "--apply"], expected);
        assert!(project.cas().verified(&garbage).unwrap());
    }
    fs::remove_file(&path).unwrap();
    let incorrect = directory.join("incorrect.json");
    fs::write(
        &incorrect,
        serde_json::to_vec(&json!({"schema-version":1,"workspace":foreign,"roots":[],"blobs":[]}))
            .unwrap(),
    )
    .unwrap();
    failure(
        &project,
        &["gc", "--apply"],
        "shared CAS root identity mismatch",
    );
    fs::remove_file(incorrect).unwrap();
    fs::create_dir(directory.join("not-a-file")).unwrap();
    failure(
        &project,
        &["gc", "--apply"],
        "invalid shared CAS root registration",
    );
    assert!(project.cas().verified(&garbage).unwrap());
}

#[test]
fn invalid_receipts_stop_gc_instead_of_silently_dropping_retention_roots() {
    let project = Fixture::new();
    project.plan(RUST_PROFILE);
    let directory = project.root.join(".isolock/receipts");
    fs::create_dir(&directory).unwrap();
    for (bytes, expected) in [
        (b"not JSON".to_vec(), "invalid receipt prevents safe GC"),
        (b"{}".to_vec(), "receipt has no manifest root"),
        (br#"{"manifest":"not-a-digest"}"#.to_vec(), "SHA-256 digest"),
        (
            serde_json::to_vec(&json!({"manifest":"0".repeat(64)})).unwrap(),
            "missing constituent manifest",
        ),
    ] {
        fs::write(directory.join("broken.json"), bytes).unwrap();
        failure(&project, &["gc", "--apply"], expected);
    }
}

#[test]
fn aliases_and_immutable_objects_cannot_be_silently_repurposed_or_overwritten() {
    let project = Fixture::new();
    let manifest = project.plan(RUST_PROFILE);
    let alias = project
        .root
        .join(".isolock/plans/rust-standalone-linux-64.lock");
    fs::write(&alias, serde_json::to_vec(&manifest).unwrap()).unwrap();
    failure(
        &project,
        &[
            "sync",
            "--profile",
            "rust-standalone",
            "--platform",
            PLATFORM,
            "--locked",
            "--offline",
        ],
        "identity does not match its alias",
    );
    fs::remove_file(alias).unwrap();
    let object = project.root.join(format!(
        ".isolock/plans/objects/{}.json",
        manifest.digest().unwrap().as_str()
    ));
    fs::write(&object, b"a corrupt pre-existing immutable object").unwrap();
    failure(
        &project,
        &["plan", "--profile", RUST_PROFILE, "--platform", PLATFORM],
        "refusing to mutate an immutable",
    );
    assert_eq!(
        fs::read(object).unwrap(),
        b"a corrupt pre-existing immutable object"
    );
}

#[test]
fn compressed_archived_seeds_are_selected_by_their_pinned_entry() {
    let mut project = Fixture::new();
    let bytes = archive(
        &[("bin/isolock", "#!/bin/sh\necho compressed-seed\n", true)],
        "-czf",
    );
    let mut seed = project.tool(
        "compressed-isolock",
        "isolock.tar.gz",
        &bytes,
        ArchiveFormat::TarGz,
    );
    seed.entry = Some("bin/isolock".into());
    let digest = seed.digest.clone();
    project
        .profiles
        .get_mut(RUST_PROFILE)
        .unwrap()
        .seeds
        .insert("isolock".into(), seed);
    project.write_config(None);
    project.plan(RUST_PROFILE);
    project.success(&[
        "seed-manifest",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
    ]);
    let manifest = fs::read_to_string(
        project
            .root
            .join(format!(".isolock/seeds/{RUST_PROFILE}-{PLATFORM}.manifest")),
    )
    .unwrap();
    assert!(manifest.contains("bin/isolock tar-gz"));
    project.sync(RUST_PROFILE);
    let owned = project
        .root
        .join(".isolock/bin/.seeds")
        .join(digest.as_str())
        .join("isolock");
    assert_eq!(
        fs::read(&owned).unwrap(),
        b"#!/bin/sh\necho compressed-seed\n"
    );
    fs::write(&owned, b"modified seed").unwrap();
    failure(
        &project,
        &[
            "sync",
            "--profile",
            RUST_PROFILE,
            "--platform",
            PLATFORM,
            "--offline",
        ],
        "provisioned seed executable was modified",
    );
}

#[test]
fn corrupt_runtime_archives_never_publish_a_success_receipt() {
    for format in [ArchiveFormat::Tar, ArchiveFormat::TarGz] {
        let mut project = Fixture::new();
        let runtime = project.tool(
            "corrupt-runtime",
            "runtime.tar.gz",
            b"not a valid archive",
            format,
        );
        project
            .profiles
            .get_mut("rust-standalone")
            .unwrap()
            .runtimes
            .insert("rust".into(), runtime);
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
        assert!(text(&output).contains("archive"), "{}", text(&output));
        assert!(!project
            .root
            .join(".isolock/receipts/rust-standalone-linux-64.json")
            .exists());
    }
}

#[test]
fn pypi_lock_pins_require_exact_versions_and_only_sha256_hashes() {
    let project = Fixture::new();
    for (line, expected) in [
        ("demo~=1.0", "exact name==version"),
        ("demo==", "platform-resolved, exact pins"),
        ("demo==1.*", "platform-resolved, exact pins"),
        ("demo==1.0;sys_platform", "platform-resolved, exact pins"),
        ("demo==1.0 --hash=sha512:abc", "only SHA-256 hashes"),
        ("demo==1.0", "require SHA-256 hashes"),
    ] {
        fs::write(project.root.join("requirements.lock"), line).unwrap();
        failure(
            &project,
            &[
                "plan",
                "--profile",
                "python-standalone",
                "--platform",
                PLATFORM,
            ],
            expected,
        );
    }
}

#[test]
fn unsupported_javascript_locks_and_shadowed_npm_locks_fail_before_planning() {
    let mut project = Fixture::new();
    for (filename, expected) in [
        ("unsupported.lock", "unsupported JavaScript lockfile name"),
        ("pnpm-lock.yaml", "invalid pnpm lockfile"),
        ("yarn.lock", "invalid Yarn lockfile"),
    ] {
        fs::write(project.root.join(filename), "not a lockfile").unwrap();
        project
            .profiles
            .get_mut("bun-standalone")
            .unwrap()
            .javascript_lock = Some(filename.into());
        project.write_config(None);
        failure(
            &project,
            &[
                "plan",
                "--profile",
                "bun-standalone",
                "--platform",
                PLATFORM,
            ],
            expected,
        );
    }
    project
        .profiles
        .get_mut("bun-standalone")
        .unwrap()
        .javascript_lock = Some("package-lock.json".into());
    project.write_config(None);
    fs::write(project.root.join("npm-shrinkwrap.json"), "{}").unwrap();
    failure(
        &project,
        &[
            "plan",
            "--profile",
            "bun-standalone",
            "--platform",
            PLATFORM,
        ],
        "shadows the selected package-lock",
    );
}

#[test]
fn non_utf8_and_escaping_lock_inputs_do_not_become_reproducible_plans() {
    let project = Fixture::new();
    fs::write(project.root.join("Cargo.lock"), b"\xFF").unwrap();
    failure(
        &project,
        &[
            "plan",
            "--profile",
            "rust-standalone",
            "--platform",
            PLATFORM,
        ],
        "not UTF-8",
    );
    fs::remove_file(project.root.join("Cargo.lock")).unwrap();
    let outside = project.temp.path().join("outside.lock");
    fs::write(&outside, b"version = 4\n").unwrap();
    std::os::unix::fs::symlink(outside, project.root.join("Cargo.lock")).unwrap();
    failure(
        &project,
        &[
            "plan",
            "--profile",
            "rust-standalone",
            "--platform",
            PLATFORM,
        ],
        "symlink escapes project root",
    );
}

#[test]
fn pixi_planning_records_the_selected_locked_wheels_and_their_python_constraint() {
    let project = Fixture::new();
    let digest = Sha256Digest::from_bytes(b"reviewed wheel");
    let url = "https://files.pythonhosted.org/demo-1.0.0-py3-none-any.whl";
    fs::write(project.root.join("pixi.lock"), format!("version: 6\nenvironments:\n  rust:\n    channels: []\n    packages:\n      linux-64:\n      - pypi: {url}\npackages:\n- pypi: {url}\n  name: demo\n  version: 1.0.0\n  sha256: {}\n  requires_python: '>=3.11'\n", digest.as_str())).unwrap();
    let manifest = project.plan(RUST_PROFILE);
    let wheel = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.name == "demo")
        .unwrap();
    assert_eq!(wheel.digest, digest);
    assert_eq!(wheel.filename, "demo-1.0.0-py3-none-any.whl");
    assert_eq!(
        wheel.metadata["requires-python"],
        Value::String(">=3.11".into())
    );
}

#[test]
fn alternate_cargo_registries_are_replaced_by_the_same_verified_offline_vendor_source() {
    let project = Fixture::new();
    let lock = project.root.join("Cargo.lock");
    let source = fs::read_to_string(&lock).unwrap().replace(
        "registry+https://github.com/rust-lang/crates.io-index",
        "registry+https://registry.example.invalid/index",
    );
    fs::write(lock, source).unwrap();
    project.plan("rust-standalone");
    project.sync("rust-standalone");
    let receipt = project.receipt("rust-standalone");
    let prefix = std::path::Path::new(receipt["prefix"].as_str().unwrap());
    let config: toml::Value =
        toml::from_str(&fs::read_to_string(prefix.join("cargo-home/config.toml")).unwrap())
            .unwrap();
    assert_eq!(config["net"]["offline"].as_bool(), Some(true));
    let replacement = config["source"]
        .as_table()
        .unwrap()
        .values()
        .find(|source| {
            source.get("registry").and_then(toml::Value::as_str)
                == Some("https://registry.example.invalid/index")
        })
        .unwrap();
    assert_eq!(replacement["replace-with"].as_str(), Some("isolock-vendor"));
}
