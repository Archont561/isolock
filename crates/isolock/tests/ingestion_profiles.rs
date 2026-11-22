//! End-to-end profile publication and restoration uses only reviewed local bytes.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid fixtures must fail the test.

mod support;

use isolock_cas::{FileStore, Sha256Digest};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use serde_json::json;
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use support::{text, Fixture, PLATFORM};

struct World {
    producer: Fixture,
    consumer: RefCell<Option<Fixture>>,
    digest: RefCell<Option<String>>,
    arguments: RefCell<Vec<String>>,
    before: RefCell<Vec<u8>>,
}

#[fixture]
fn world() -> World {
    World {
        producer: Fixture::new(),
        consumer: RefCell::new(None),
        digest: RefCell::new(None),
        arguments: RefCell::new(vec![
            "push".into(),
            "--profile".into(),
            "rust-standalone".into(),
            "--platform".into(),
            PLATFORM.into(),
        ]),
        before: RefCell::new(Vec::new()),
    }
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

fn remote(fixture: &Fixture) -> PathBuf {
    fixture.temp.path().join("remote.git")
}

fn configure(fixture: &Fixture, source: &Path) {
    fixture.write_config(Some(source));
    let path = fixture.root.join("isolock.toml");
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config
        .get_mut("storage")
        .unwrap()
        .as_table_mut()
        .unwrap()
        .insert("transport".into(), toml::Value::String("git".into()));
    fs::write(path, toml::to_string(&config).unwrap()).unwrap();
}

#[given("reviewed originals for a locked standalone Rust profile")]
fn reviewed_profile(world: &World) {
    let fixture = &world.producer;
    let originals = fixture.root.join("originals");
    fs::create_dir(&originals).unwrap();
    let mut artifacts = Vec::new();
    for (name, bytes) in &fixture.blobs {
        let filename = format!("{name}.bin");
        fs::write(originals.join(&filename), bytes).unwrap();
        artifacts.push(json!({"registry":"generic", "name":name, "version":"1", "filename":filename,
            "source":format!("https://github.com/example/reviewed/{name}.bin"), "sha256":Sha256Digest::from_bytes(bytes)}));
    }
    // Profile input originals are also byte-pinned in the payload commit. They are
    // not implicitly trusted just because a later manifest references their digest.
    let bytes = fs::read(fixture.root.join("Cargo.lock")).unwrap();
    fs::write(originals.join("Cargo.lock"), &bytes).unwrap();
    artifacts.push(json!({"registry":"generic", "name":"cargo-lock", "version":"1", "filename":"Cargo.lock",
        "source":"https://github.com/example/reviewed/Cargo.lock", "sha256":Sha256Digest::from_bytes(&bytes)}));
    fs::write(
        fixture.root.join("reviewed.json"),
        serde_json::to_vec_pretty(&json!({"schema-version":1,"artifacts":artifacts})).unwrap(),
    )
    .unwrap();
    fs::remove_dir_all(fixture.cas().root()).unwrap();
    git(&fixture.root, &["init", "--quiet", "--initial-branch=main"]);
    git(
        &fixture.root,
        &["config", "user.name", "Reviewed publisher"],
    );
    git(
        &fixture.root,
        &["config", "user.email", "publisher@example.invalid"],
    );
    git(
        fixture.temp.path(),
        &["init", "--quiet", "--bare", "remote.git"],
    );
    git(
        &fixture.root,
        &["remote", "add", "origin", remote(fixture).to_str().unwrap()],
    );
    configure(fixture, &remote(fixture));
}

#[when("its payloads and pinned profile manifest are explicitly published")]
fn publish_profile(world: &World) {
    let fixture = &world.producer;
    fixture.success(&[
        "ingest",
        "--manifest",
        "reviewed.json",
        "--artifacts",
        "originals",
    ]);
    fixture.success(&["push", "--manifest", "reviewed.json"]);
    let payload = git(&remote(fixture), &["rev-parse", "isolock/v1"]);
    let manifest = fixture.plan("rust-standalone");
    assert_eq!(
        manifest.storage.commit.as_deref(),
        Some(std::str::from_utf8(&payload).unwrap().trim())
    );
    fixture.success(&[
        "push",
        "--profile",
        "rust-standalone",
        "--platform",
        PLATFORM,
    ]);
    assert_eq!(
        git(&remote(fixture), &["rev-parse", "isolock/v1^"]),
        payload
    );
    *world.digest.borrow_mut() = Some(manifest.digest().unwrap().as_str().into());
}

#[then("a cold consumer fetches only its selected verified closure")]
fn cold_profile(world: &World) {
    let mut consumer = Fixture::new();
    consumer.profiles = world.producer.profiles.clone();
    for file in [
        "Cargo.lock",
        "Cargo.toml",
        "catalog.json",
        "pixi.lock",
        "pixi.toml",
        "package.json",
        "package-lock.json",
        "requirements.lock",
    ] {
        fs::copy(world.producer.root.join(file), consumer.root.join(file)).unwrap();
    }
    configure(&consumer, &remote(&world.producer));
    fs::remove_dir_all(consumer.cas().root()).unwrap();
    assert!(!consumer.cas().root().exists());
    let digest = world.digest.borrow();
    consumer.success(&[
        "fetch",
        "--manifest",
        digest.as_ref().unwrap(),
        "--profile",
        "rust-standalone",
        "--platform",
        PLATFORM,
    ]);
    let manifest = consumer.manifest("rust-standalone");
    let stored = FileStore::new(consumer.cas().root()).digests().unwrap();
    assert_eq!(stored, manifest.reachable_blobs());
    *world.consumer.borrow_mut() = Some(consumer);
}

#[then("the selected runtime runs offline after the storage remote disappears")]
fn offline_runtime(world: &World) {
    fs::remove_dir_all(remote(&world.producer)).unwrap();
    fs::remove_dir_all(world.producer.root.join("originals")).unwrap();
    let consumer = world.consumer.borrow();
    let consumer = consumer.as_ref().unwrap();
    consumer.sync("rust-standalone");
    let output = Command::new(consumer.bin("rust-standalone", "cargo"))
        .arg("build")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "cargo fixture: built hello offline\n"
    );
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "A published profile restores a cold offline runtime"
)]
fn published_profile_restores_offline(world: World) {
    let _ = world;
}

#[given("profile publication has {problem:string}")]
fn invalid_profile_publication(world: &World, problem: String) {
    let fixture = &world.producer;
    if problem == "unpublished input" {
        let path = fixture.root.join("reviewed.json");
        let mut review: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        review["artifacts"]
            .as_array_mut()
            .unwrap()
            .retain(|artifact| artifact["name"] != "cargo-lock");
        fs::write(path, serde_json::to_vec(&review).unwrap()).unwrap();
    }
    fixture.success(&[
        "ingest",
        "--manifest",
        "reviewed.json",
        "--artifacts",
        "originals",
    ]);
    fixture.success(&["push", "--manifest", "reviewed.json"]);
    if problem == "directory plan" {
        let path = fixture.root.join("isolock.toml");
        let contents = fs::read_to_string(&path)
            .unwrap()
            .replace("transport = \"git\"", "transport = \"directory\"");
        fs::write(path, contents).unwrap();
    }
    if problem == "corrupt pinned blob" {
        let clone = tempfile::tempdir().unwrap();
        git(
            clone.path(),
            &[
                "clone",
                "--quiet",
                "--branch",
                "isolock/v1",
                remote(fixture).to_str().unwrap(),
                ".",
            ],
        );
        git(clone.path(), &["config", "user.name", "Other publisher"]);
        git(
            clone.path(),
            &["config", "user.email", "other@example.invalid"],
        );
        let digest = Sha256Digest::from_bytes(&fixture.blobs["cargo"]);
        fs::write(
            clone.path().join("cas").join(digest.sharded_path()),
            b"corrupt mirrored blob",
        )
        .unwrap();
        git(clone.path(), &["add", "."]);
        git(
            clone.path(),
            &["commit", "--quiet", "-m", "test: corrupt remote snapshot"],
        );
        git(
            clone.path(),
            &["push", "--quiet", "origin", "HEAD:refs/heads/isolock/v1"],
        );
    }
    fixture.plan("rust-standalone");
    match problem.as_str() {
        "missing profile" => {
            *world.arguments.borrow_mut() =
                vec!["push".into(), "--platform".into(), PLATFORM.into()]
        }
        "missing platform" => {
            *world.arguments.borrow_mut() =
                vec!["push".into(), "--profile".into(), "rust-standalone".into()]
        }
        "mixed package flags" => world
            .arguments
            .borrow_mut()
            .extend(["--registry".into(), "cargo".into()]),
        "different destination" => {
            git(
                fixture.temp.path(),
                &["init", "--quiet", "--bare", "other.git"],
            );
            world.arguments.borrow_mut().extend([
                "--remote".into(),
                fixture
                    .temp
                    .path()
                    .join("other.git")
                    .to_str()
                    .unwrap()
                    .into(),
            ]);
        }
        "input drift" => fs::write(fixture.root.join("Cargo.lock"), "changed lockfile").unwrap(),
        "directory plan" | "unpublished input" | "corrupt pinned blob" => (),
        _ => panic!("unknown profile problem: {problem}"),
    }
    *world.before.borrow_mut() = git(
        &remote(fixture),
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    );
}

#[when("the selected profile is published")]
fn attempt_profile_publication(world: &World) {
    let args = world.arguments.borrow();
    let output = world
        .producer
        .run(&args.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(
        output.stdout.is_empty(),
        "a failed publication must not report success: {}",
        text(&output)
    );
    *world.digest.borrow_mut() = Some(String::from_utf8_lossy(&output.stderr).into_owned());
}

#[then("profile publication fails with {diagnostic:string} without changing storage")]
fn unchanged_profile_storage(world: &World, diagnostic: String) {
    assert!(
        world
            .digest
            .borrow()
            .as_ref()
            .unwrap()
            .contains(&diagnostic),
        "{:?}",
        world.digest.borrow()
    );
    assert_eq!(
        git(
            &remote(&world.producer),
            &["for-each-ref", "--format=%(refname) %(objectname)"]
        ),
        *world.before.borrow()
    );
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Profile publication cannot repair or change a locked payload history"
)]
fn profile_publication_respects_locked_history(world: World, problem: String, diagnostic: String) {
    let _ = (world, problem, diagnostic);
}
