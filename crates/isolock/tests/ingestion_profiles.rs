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
    retained: RefCell<Vec<String>>,
    migration_source: RefCell<String>,
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
        retained: RefCell::new(Vec::new()),
        migration_source: RefCell::new("isolock/v1".into()),
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
    cold_profile_from(world, "isolock/v1");
}

fn cold_profile_from(world: &World, reference: &str) {
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
    let path = consumer.root.join("isolock.toml");
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["storage"]
        .as_table_mut()
        .unwrap()
        .insert("reference".into(), toml::Value::String(reference.into()));
    fs::write(path, toml::to_string(&config).unwrap()).unwrap();
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
        "mixed epoch policy" => world
            .arguments
            .borrow_mut()
            .extend(["--epoch-policy".into(), "reviewed.json".into()]),
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

#[when("reviewed history pressure rolls the profile into a new epoch")]
fn rollover_profile(world: &World) {
    rollover_profile_to(world, "isolock/v1", "isolock/v2");
}

fn rollover_profile_to(world: &World, source_ref: &str, next_ref: &str) {
    let fixture = &world.producer;
    // A separate retired batch is not a retention root. Its bytes force a rollover
    // while leaving ample room for the complete mandatory profile closure.
    for (name, bytes) in [("retired", vec![b'x'; 65536]), ("incoming", vec![b'y'; 64])] {
        fs::write(fixture.root.join(format!("originals/{name}.bin")), &bytes).unwrap();
        fs::write(fixture.root.join(format!("{name}.json")), serde_json::to_vec_pretty(&json!({
            "schema-version":1,"artifacts":[{"registry":"generic","name":name,"version":"1",
            "filename":format!("{name}.bin"),"source":format!("https://github.com/example/{name}.bin"),
            "sha256":Sha256Digest::from_bytes(&bytes)}]
        })).unwrap()).unwrap();
        fixture.success(&[
            "ingest",
            "--manifest",
            &format!("{name}.json"),
            "--artifacts",
            "originals",
        ]);
        if name == "retired" {
            fixture.success(&[
                "push",
                "--manifest",
                "retired.json",
                "--storage-ref",
                source_ref,
            ]);
        }
    }
    let source = git(&remote(fixture), &["rev-parse", source_ref]);
    let live: u64 = String::from_utf8(git(&remote(fixture), &["ls-tree", "-rl", source_ref]))
        .unwrap()
        .lines()
        .map(|line| {
            line.split_whitespace()
                .nth(3)
                .unwrap()
                .parse::<u64>()
                .unwrap()
        })
        .sum();
    fs::write(fixture.root.join("epoch-policy.json"), serde_json::to_vec_pretty(&json!({
        "schema-version":1,"source-ref":source_ref,"source-commit":std::str::from_utf8(&source).unwrap().trim(),
        "next-ref":next_ref,"max-live-bytes":live-1,"max-history-bytes":live-1,
        "retain-batches":[],"retain-manifests":*world.retained.borrow()
    })).unwrap()).unwrap();
    fixture.success(&[
        "push",
        "--manifest",
        "incoming.json",
        "--storage-ref",
        source_ref,
        "--epoch-policy",
        "epoch-policy.json",
    ]);
}

#[then("the successor preserves immutable profiles input originals and provenance")]
fn retained_profile(world: &World) {
    let fixture = &world.producer;
    let manifest = fixture.manifest("rust-standalone");
    let mut paths = String::from_utf8(git(
        &remote(fixture),
        &["ls-tree", "-r", "--name-only", "isolock/v1", "--", "plans/"],
    ))
    .unwrap()
    .lines()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    paths.extend(manifest.reachable_blobs().iter().map(|digest| {
        format!(
            "cas/{}",
            Sha256Digest::parse(digest).unwrap().sharded_path()
        )
    }));
    for path in paths {
        assert_eq!(
            git(&remote(fixture), &["show", &format!("isolock/v2:{path}")]),
            git(&remote(fixture), &["show", &format!("isolock/v1:{path}")])
        );
    }
    let audits = String::from_utf8(git(
        &remote(fixture),
        &[
            "ls-tree",
            "-r",
            "--name-only",
            "isolock/v1",
            "--",
            "ingestion/",
        ],
    ))
    .unwrap();
    let required = manifest.reachable_blobs();
    for path in audits.lines() {
        let bytes = git(&remote(fixture), &["show", &format!("isolock/v1:{path}")]);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let matches = if path.contains("/records/") {
            required.contains(value["sha256"].as_str().unwrap())
        } else {
            value["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|artifact| required.contains(artifact["sha256"].as_str().unwrap()))
        };
        if matches {
            assert_eq!(
                git(&remote(fixture), &["show", &format!("isolock/v2:{path}")]),
                bytes
            );
        }
    }
    assert_eq!(
        git(&remote(fixture), &["rev-list", "--count", "isolock/v2"]),
        b"1\n"
    );
    let paths = git(
        &remote(fixture),
        &["ls-tree", "-r", "--name-only", "isolock/v2"],
    );
    assert!(!String::from_utf8_lossy(&paths)
        .contains(Sha256Digest::from_bytes(&vec![b'x'; 65536]).as_str()));
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Rollover preserves locked profiles for cold offline consumers"
)]
fn rollover_profile_restores_offline(world: World) {
    let _ = world;
}

#[when("a composed profile and an unaliased immutable root are retained")]
fn composed_root(world: &World) {
    let fixture = &world.producer;
    world
        .retained
        .borrow_mut()
        .push(world.digest.borrow().as_ref().unwrap().clone());
    let path = fixture.root.join("isolock.toml");
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["profiles"].as_table_mut().unwrap().insert(
        "suite".into(),
        toml::Value::try_from(
            json!({"provider":"composed","platforms":[PLATFORM],"compose":["rust-standalone"]}),
        )
        .unwrap(),
    );
    fs::write(path, toml::to_string(&config).unwrap()).unwrap();
    fixture.plan("suite");
    fixture.success(&["push", "--profile", "suite", "--platform", PLATFORM]);
    assert_ne!(
        fixture
            .manifest("rust-standalone")
            .digest()
            .unwrap()
            .as_str(),
        world.digest.borrow().as_ref().unwrap()
    );
}

#[then("another rollover retains the original locked payload history")]
fn second_rollover(world: &World) {
    rollover_profile_to(world, "isolock/v2", "isolock/v3");
    let fixture = &world.producer;
    assert_eq!(
        git(&remote(fixture), &["rev-list", "--count", "isolock/v3"]),
        b"1\n"
    );
    let paths = String::from_utf8(git(
        &remote(fixture),
        &["ls-tree", "-r", "--name-only", "isolock/v2", "--", "plans/"],
    ))
    .unwrap();
    assert!(paths.contains(world.digest.borrow().as_ref().unwrap()));
    for path in paths.lines() {
        assert_eq!(
            git(&remote(fixture), &["show", &format!("isolock/v2:{path}")]),
            git(&remote(fixture), &["show", &format!("isolock/v3:{path}")])
        );
    }
    let bytes = git(
        &remote(fixture),
        &[
            "show",
            &format!(
                "isolock/v3:plans/objects/{}.json",
                world.digest.borrow().as_ref().unwrap()
            ),
        ],
    );
    let manifest: isolock_plan::profile::ProfileManifest = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(manifest.storage.reference.as_deref(), Some("isolock/v1"));
    let commit = manifest.storage.commit.unwrap();
    let status = Command::new("git")
        .arg("-C")
        .arg(remote(fixture))
        .args(["merge-base", "--is-ancestor", &commit, "isolock/v1"])
        .status()
        .unwrap();
    assert!(status.success());
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Composed and immutable profile roots survive successive epochs"
)]
fn profile_graph_survives_multiple_epochs(world: World) {
    let _ = world;
}

fn edit_profile_snapshot(fixture: &Fixture, edit: impl FnOnce(&Path)) {
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
    git(
        clone.path(),
        &["config", "user.name", "Changed profile fixture"],
    );
    git(
        clone.path(),
        &["config", "user.email", "changed@example.invalid"],
    );
    edit(clone.path());
    git(clone.path(), &["add", "-A"]);
    git(
        clone.path(),
        &["commit", "--quiet", "-m", "test: changed retained profile"],
    );
    git(
        clone.path(),
        &["push", "--quiet", "origin", "HEAD:refs/heads/isolock/v1"],
    );
}

#[when("retained profile migration has {problem:string}")]
fn retained_profile_problem(world: &World, problem: String) {
    let fixture = &world.producer;
    let manifest = fixture.manifest("rust-standalone");
    let alias = format!("plans/rust-standalone-{PLATFORM}.lock");
    let object = format!("plans/objects/{}.json", manifest.digest().unwrap().as_str());
    match problem.as_str() {
        "missing immutable root" => {
            edit_profile_snapshot(fixture, |root| fs::remove_file(root.join(object)).unwrap())
        }
        "changed immutable root" => {
            edit_profile_snapshot(fixture, |root| fs::write(root.join(object), b"{}").unwrap())
        }
        "wrong alias identity" => edit_profile_snapshot(fixture, |root| {
            fs::rename(root.join(alias), root.join("plans/wrong.lock")).unwrap()
        }),
        "unsupported profile schema" => edit_profile_snapshot(fixture, |root| {
            let path = root.join(alias);
            let mut value: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            value["schema-version"] = json!(99);
            fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        }),
        "missing profile input" => edit_profile_snapshot(fixture, |root| {
            fs::remove_file(root.join(format!(
                "cas/{}",
                manifest.inputs["Cargo.lock"].sharded_path()
            )))
            .unwrap()
        }),
        "corrupt profile payload" => edit_profile_snapshot(fixture, |root| {
            fs::write(
                root.join(format!(
                    "cas/{}",
                    Sha256Digest::from_bytes(&fixture.blobs["cargo"]).sharded_path()
                )),
                b"corrupt retained payload",
            )
            .unwrap()
        }),
        "missing profile provenance" => edit_profile_snapshot(fixture, |root| {
            for entry in fs::read_dir(root.join("ingestion/records")).unwrap() {
                let path = entry.unwrap().path();
                let record: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                if record["sha256"] == json!(manifest.inputs["Cargo.lock"]) {
                    fs::remove_file(path).unwrap();
                }
            }
        }),
        "detached locked payload history" => {
            git(
                &remote(fixture),
                &["config", "user.name", "Detached history fixture"],
            );
            git(
                &remote(fixture),
                &["config", "user.email", "detached@example.invalid"],
            );
            let tree = git(&remote(fixture), &["rev-parse", "isolock/v1^{tree}"]);
            let detached = git(
                &remote(fixture),
                &[
                    "commit-tree",
                    std::str::from_utf8(&tree).unwrap().trim(),
                    "-m",
                    "test: detached payload history",
                ],
            );
            git(
                &remote(fixture),
                &[
                    "update-ref",
                    "refs/heads/isolock/v1",
                    std::str::from_utf8(&detached).unwrap().trim(),
                ],
            );
        }
        "missing archived payload ref" => {
            rollover_profile(world);
            git(
                &remote(fixture),
                &["update-ref", "-d", "refs/heads/isolock/v1"],
            );
            *world.migration_source.borrow_mut() = "isolock/v2".into();
        }
        _ => panic!("unknown retained profile problem {problem}"),
    }
}

#[when("a reviewed rollover of that retained profile is attempted")]
fn reject_damaged_retained_profile(world: &World) {
    let fixture = &world.producer;
    // A distinct incoming review must not explicitly repair the damaged closure.
    let bytes = b"separate incoming publication";
    fs::write(fixture.root.join("originals/next.bin"), bytes).unwrap();
    fs::write(
        fixture.root.join("next.json"),
        serde_json::to_vec(&json!({"schema-version":1,"artifacts":[{
            "registry":"generic","name":"next","version":"1","filename":"next.bin",
            "source":"https://github.com/example/next.bin","sha256":Sha256Digest::from_bytes(bytes)
        }]}))
        .unwrap(),
    )
    .unwrap();
    fixture.success(&[
        "ingest",
        "--manifest",
        "next.json",
        "--artifacts",
        "originals",
    ]);
    let source = world.migration_source.borrow();
    let next = if source.as_str() == "isolock/v1" {
        "isolock/v2"
    } else {
        "isolock/v3"
    };
    let commit = git(&remote(fixture), &["rev-parse", &source]);
    fs::write(fixture.root.join("epoch-policy.json"), serde_json::to_vec(&json!({
        "schema-version":1,"source-ref":*source,"source-commit":std::str::from_utf8(&commit).unwrap().trim(),
        "next-ref":next,"max-live-bytes":1,"max-history-bytes":1,"retain-batches":[],"retain-manifests":[]
    })).unwrap()).unwrap();
    *world.before.borrow_mut() = git(
        &remote(fixture),
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    );
    let output = fixture.run(&[
        "push",
        "--manifest",
        "next.json",
        "--epoch-policy",
        "epoch-policy.json",
        "--storage-ref",
        &source,
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(output.stdout.is_empty(), "{}", text(&output));
    *world.digest.borrow_mut() = Some(String::from_utf8_lossy(&output.stderr).into_owned());
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Damaged retained profiles cannot be migrated"
)]
fn damaged_retained_profile_fails_closed(world: World, problem: String, diagnostic: String) {
    let _ = (world, problem, diagnostic);
}

#[then("the old lock cannot silently adopt the successor reference")]
fn explicit_adoption_required(world: &World) {
    let fixture = &world.producer;
    let path = fixture
        .root
        .join(format!(".isolock/plans/rust-standalone-{PLATFORM}.lock"));
    let before = fs::read(&path).unwrap();
    let config = fs::read(fixture.root.join("isolock.toml")).unwrap();
    let output = fixture.run(&[
        "sync",
        "--profile",
        "rust-standalone",
        "--platform",
        PLATFORM,
        "--locked",
        "--offline",
        "--storage-ref",
        "isolock/v2",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("locked input changed"),
        "{}",
        text(&output)
    );
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(fs::read(fixture.root.join("isolock.toml")).unwrap(), config);
}

#[when("the profile is explicitly replanned and published on the successor")]
fn adopt_successor(world: &World) {
    let fixture = &world.producer;
    let config = fs::read(fixture.root.join("isolock.toml")).unwrap();
    let old_alias = git(
        &remote(fixture),
        &[
            "show",
            &format!("isolock/v1:plans/rust-standalone-{PLATFORM}.lock"),
        ],
    );
    let payload = git(&remote(fixture), &["rev-parse", "isolock/v2"]);
    fixture.success(&[
        "plan",
        "--profile",
        "rust-standalone",
        "--platform",
        PLATFORM,
        "--storage-ref",
        "isolock/v2",
    ]);
    let manifest = fixture.manifest("rust-standalone");
    assert_eq!(manifest.storage.reference.as_deref(), Some("isolock/v2"));
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
        "--storage-ref",
        "isolock/v2",
    ]);
    assert_eq!(
        git(
            &remote(fixture),
            &[
                "show",
                &format!("isolock/v1:plans/rust-standalone-{PLATFORM}.lock")
            ]
        ),
        old_alias
    );
    assert_eq!(fs::read(fixture.root.join("isolock.toml")).unwrap(), config);
    *world.digest.borrow_mut() = Some(manifest.digest().unwrap().as_str().into());
}

#[then("a cold successor consumer fetches the selected verified closure")]
fn cold_successor_consumer(world: &World) {
    cold_profile_from(world, "isolock/v2");
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Adopting a successor reference requires an explicit profile replan"
)]
fn explicit_successor_adoption_restores_offline(world: World) {
    let _ = world;
}

#[then("retained profile migration fails with {diagnostic:string} without changing storage")]
fn unchanged_retained_profile_storage(world: &World, diagnostic: String) {
    unchanged_profile_storage(world, diagnostic);
}
