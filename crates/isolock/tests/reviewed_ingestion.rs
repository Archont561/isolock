//! Real CLI, public CAS layout, Git refs, and cold consumers are the test seams.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid fixtures must fail the test.

mod coverage_support;

use isolock_cas::{FileStore, Sha256Digest};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use serde_json::json;
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const ORIGINAL: &[u8] = b"reviewed original archive bytes\n";

struct Checkout {
    head: Vec<u8>,
    status: Vec<u8>,
    index: Vec<u8>,
}

struct World {
    temp: TempDir,
    output: RefCell<Option<Output>>,
    checkout: RefCell<Option<Checkout>>,
    remote_before: RefCell<Vec<u8>>,
    arguments: RefCell<Vec<String>>,
}

#[fixture]
fn world() -> World {
    World {
        temp: tempfile::tempdir().unwrap(),
        output: RefCell::new(None),
        checkout: RefCell::new(None),
        remote_before: RefCell::new(Vec::new()),
        arguments: RefCell::new(vec![
            "push".into(),
            "--manifest".into(),
            "reviewed.json".into(),
        ]),
    }
}

impl World {
    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", self.temp.path().join("empty-home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .current_dir(self.temp.path())
            .args(arguments);
        coverage_support::preserve_profile(&mut command);
        command
    }
    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().unwrap()
    }
    fn cas(&self) -> FileStore {
        FileStore::new(self.temp.path().join("custom-cas"))
    }
    fn manifest(&self) -> PathBuf {
        self.temp.path().join("reviewed.json")
    }
}

fn success(output: &Output) {
    assert!(output.status.success(), "{output:?}");
}

#[given("a reviewed artifact manifest and its original archive")]
fn reviewed_artifact(world: &World) {
    fs::create_dir(world.temp.path().join("originals")).unwrap();
    fs::write(
        world.temp.path().join("originals/demo-1.0.0.crate"),
        ORIGINAL,
    )
    .unwrap();
    fs::write(
        world.manifest(),
        serde_json::to_vec_pretty(&json!({
            "schema-version": 1,
            "artifacts": [{
                "registry": "cargo", "name": "demo", "version": "1.0.0",
                "sha256": Sha256Digest::from_bytes(ORIGINAL),
                "filename": "demo-1.0.0.crate",
                "source": "https://github.com/example/reviewed/demo-1.0.0.crate"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        world.temp.path().join("isolock.toml"),
        "[storage]\ncas-dir = 'custom-cas'\n",
    )
    .unwrap();
}

#[when("the reviewed manifest is ingested through the CLI")]
fn ingest(world: &World) {
    *world.output.borrow_mut() = Some(world.run(&[
        "ingest",
        "--manifest",
        "reviewed.json",
        "--artifacts",
        "originals",
    ]));
}

#[then("the original bytes are verified in the configured CAS")]
fn verified_cas(world: &World) {
    let output = world.output.borrow();
    success(output.as_ref().unwrap());
    let digest = Sha256Digest::from_bytes(ORIGINAL);
    assert!(world.cas().verified(&digest).unwrap());
    assert_eq!(fs::read(world.cas().path(&digest)).unwrap(), ORIGINAL);
    assert!(String::from_utf8_lossy(&output.as_ref().unwrap().stdout)
        .contains("Verified 1 reviewed artifact"));
}

#[then("no storage reference is published")]
fn no_publication(world: &World) {
    assert!(!world.temp.path().join(".git").exists());
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Reviewed local bytes are verified before entering CAS"
)]
fn reviewed_local_ingestion(world: World) {
    let _ = world;
}

fn git(root: &Path, arguments: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", root.join("empty-home"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-C"])
        .arg(root)
        .args(arguments)
        .output()
        .unwrap();
    success(&output);
    output.stdout
}

#[given("a publisher checkout with staged and unstaged code changes")]
fn publisher(world: &World) {
    let root = world.temp.path();
    git(root, &["init", "--quiet", "--initial-branch=main"]);
    git(root, &["config", "user.name", "Reviewed publisher"]);
    git(root, &["config", "user.email", "publisher@example.invalid"]);
    fs::write(
        root.join(".gitignore"),
        "originals/\ncustom-cas/\n.isolock/\nremote.git/\nconsumer/\n",
    )
    .unwrap();
    fs::write(root.join("code.txt"), "original code\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "test: initial code"]);
    fs::write(root.join("code.txt"), "staged code\n").unwrap();
    git(root, &["add", "code.txt"]);
    fs::write(root.join("code.txt"), "unstaged code\n").unwrap();
    fs::write(root.join("untracked.txt"), "untracked code\n").unwrap();
    git(root, &["init", "--quiet", "--bare", "remote.git"]);
    git(root, &["remote", "add", "origin", "remote.git"]);
    capture_checkout(world);
}

fn capture_checkout(world: &World) {
    let root = world.temp.path();
    *world.checkout.borrow_mut() = Some(Checkout {
        head: git(root, &["rev-parse", "HEAD"]),
        status: git(root, &["status", "--porcelain=v1", "-z"]),
        index: fs::read(root.join(".git/index")).unwrap(),
    });
}

#[when("the reviewed manifest is ingested and explicitly published")]
fn ingest_and_publish(world: &World) {
    ingest(world);
    success(world.output.borrow().as_ref().unwrap());
    *world.output.borrow_mut() = Some(world.run(&["push", "--manifest", "reviewed.json"]));
}

#[then("the storage remote contains the reviewed bytes and audit commit")]
fn published(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    let remote = world.temp.path().join("remote.git");
    let blob = format!(
        "isolock/v1:cas/{}",
        Sha256Digest::from_bytes(ORIGINAL).sharded_path()
    );
    assert_eq!(git(&remote, &["show", &blob]), ORIGINAL);
    assert_eq!(
        git(&remote, &["log", "-1", "--format=%s", "isolock/v1"]),
        b"chore(deps): add cargo/demo==1.0.0\n"
    );
    assert_eq!(git(&remote, &["rev-list", "--count", "isolock/v1"]), b"1\n");
    let plan: isolock_plan::AirlockPlan =
        serde_json::from_slice(&git(&remote, &["show", "isolock/v1:plan.json"])).unwrap();
    assert_eq!(
        plan.artifacts()[0].digest,
        Sha256Digest::from_bytes(ORIGINAL)
    );
    assert_eq!(
        plan.artifact_metadata(0).unwrap().filename.as_deref(),
        Some("demo-1.0.0.crate")
    );
}

#[then("the producer branch, index, and working files are unchanged")]
fn unchanged_checkout(world: &World) {
    let root = world.temp.path();
    let saved = world.checkout.borrow();
    let Checkout {
        head,
        status,
        index,
    } = saved.as_ref().unwrap();
    assert_eq!(&git(root, &["rev-parse", "HEAD"]), head);
    assert_eq!(&git(root, &["status", "--porcelain=v1", "-z"]), status);
    assert_eq!(&fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join("code.txt")).unwrap(), b"unstaged code\n");
    assert_eq!(git(root, &["branch", "--show-current"]), b"main\n");
}

#[then("a cold consumer verifies the snapshot without the originals")]
fn cold_consumer(world: &World) {
    fs::remove_dir_all(world.temp.path().join("originals")).unwrap();
    git(
        world.temp.path(),
        &[
            "clone",
            "--quiet",
            "--no-local",
            "--single-branch",
            "--branch",
            "isolock/v1",
            "remote.git",
            "consumer",
        ],
    );
    let output = world
        .command(&["plan", "--ci", "--cas-dir", "cas"])
        .current_dir(world.temp.path().join("consumer"))
        .env("ISOLOCK_PLAN", "plan.json")
        .output()
        .unwrap();
    success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("ALL DEPENDENCIES SATISFIED"));
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Explicit publication preserves the code checkout and restores offline"
)]
fn explicit_publication_and_cold_consumer(world: World) {
    let _ = world;
}

fn change_manifest(world: &World, edit: impl FnOnce(&mut serde_json::Value)) {
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(world.manifest()).unwrap()).unwrap();
    edit(&mut manifest);
    fs::write(
        world.manifest(),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

#[given("the ingestion input has {problem:string}")]
fn ingestion_problem(world: &World, problem: String) {
    let original = world.temp.path().join("originals/demo-1.0.0.crate");
    match problem.as_str() {
        "unsupported schema" => change_manifest(world, |m| m["schema-version"] = json!(2)),
        "unknown field" => {
            change_manifest(world, |m| m["secret"] = json!("do-not-echo-this-secret"))
        }
        "empty batch" => change_manifest(world, |m| m["artifacts"] = json!([])),
        "duplicate identity" => change_manifest(world, |m| {
            let item = m["artifacts"][0].clone();
            m["artifacts"].as_array_mut().unwrap().push(item);
        }),
        "duplicate filename" => change_manifest(world, |m| {
            let mut item = m["artifacts"][0].clone();
            item["name"] = json!("other");
            m["artifacts"].as_array_mut().unwrap().push(item);
        }),
        "invalid registry" => {
            change_manifest(world, |m| m["artifacts"][0]["registry"] = json!("unknown"))
        }
        "traversal filename" => change_manifest(world, |m| {
            m["artifacts"][0]["filename"] = json!("../demo.crate")
        }),
        "invalid package name" => {
            change_manifest(world, |m| m["artifacts"][0]["name"] = json!("$(malicious)"))
        }
        "invalid version" => change_manifest(world, |m| {
            m["artifacts"][0]["version"] = json!("1.0.0\ninjected")
        }),
        "malformed digest" => change_manifest(world, |m| {
            m["artifacts"][0]["sha256"] = json!("not-a-sha256")
        }),
        "conflicting filename" => change_manifest(world, |m| {
            m["artifacts"][0]["metadata"] = json!({"filename":"other.crate"})
        }),
        "credential source" => change_manifest(world, |m| {
            m["artifacts"][0]["source"] =
                json!("https://user:do-not-echo-this-secret@example.invalid/artifact")
        }),
        "non-HTTPS source" => change_manifest(world, |m| {
            m["artifacts"][0]["source"] = json!("http://example.invalid/artifact")
        }),
        "whitespace source" => change_manifest(world, |m| {
            m["artifacts"][0]["source"] = json!("https://example.invalid/artifact with spaces")
        }),
        "missing original" => fs::remove_file(original).unwrap(),
        "directory original" => {
            fs::remove_file(&original).unwrap();
            fs::create_dir(original).unwrap();
        }
        "symlink original" => {
            fs::rename(&original, original.with_extension("saved")).unwrap();
            std::os::unix::fs::symlink(original.with_extension("saved"), original).unwrap();
        }
        "symlink directory" => {
            let root = world.temp.path();
            fs::rename(root.join("originals"), root.join("saved-originals")).unwrap();
            std::os::unix::fs::symlink(root.join("saved-originals"), root.join("originals"))
                .unwrap();
        }
        "mismatched original" => fs::write(original, b"unreviewed bytes").unwrap(),
        "later mismatched original" => {
            fs::write(
                world.temp.path().join("originals/other.crate"),
                b"unreviewed bytes",
            )
            .unwrap();
            change_manifest(world, |m| {
                let mut item = m["artifacts"][0].clone();
                item["name"] = json!("other");
                item["filename"] = json!("other.crate");
                m["artifacts"].as_array_mut().unwrap().push(item);
            });
        }
        "corrupt existing CAS" => {
            let digest = Sha256Digest::from_bytes(ORIGINAL);
            world.cas().put_bytes(&digest, ORIGINAL).unwrap();
            fs::write(world.cas().path(&digest), b"existing corruption").unwrap();
        }
        "oversized original" => fs::OpenOptions::new()
            .write(true)
            .open(original)
            .unwrap()
            .set_len(95 * 1024 * 1024 + 1)
            .unwrap(),
        "missing review" => fs::remove_file(world.manifest()).unwrap(),
        "oversized encoded review" => {
            change_manifest(world, |review| {
                review["artifacts"][0]["metadata"] = json!({"registry_metadata":vec!["x"; 400_000]})
            });
            let review: serde_json::Value =
                serde_json::from_slice(&fs::read(world.manifest()).unwrap()).unwrap();
            fs::write(world.manifest(), serde_json::to_vec(&review).unwrap()).unwrap();
        }
        "oversized review" => fs::write(world.manifest(), vec![b' '; 4 * 1024 * 1024 + 1]).unwrap(),
        _ => panic!("unknown ingestion problem: {problem}"),
    }
}

#[then("ingestion fails with {diagnostic:string} without accepting the batch")]
fn rejected_ingestion(world: &World, diagnostic: String) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&diagnostic), "{stderr}");
    assert!(!stderr.contains("do-not-echo-this-secret"), "{stderr}");
    let digest = Sha256Digest::from_bytes(ORIGINAL);
    if world.cas().path(&digest).exists() {
        assert_eq!(
            fs::read(world.cas().path(&digest)).unwrap(),
            b"existing corruption"
        );
    } else {
        assert!(world.cas().digests().unwrap().is_empty());
    }
    assert!(!world
        .temp
        .path()
        .join(".isolock/ingestion/objects")
        .exists());
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Rejected reviews never publish a partial batch"
)]
fn rejected_review_is_not_a_partial_import(world: World, problem: String, diagnostic: String) {
    let _ = (world, problem, diagnostic);
}

fn storage_refs(world: &World) -> Vec<u8> {
    git(
        &world.temp.path().join("remote.git"),
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    )
}

fn append_review(world: &World) {
    fs::write(
        world.temp.path().join("originals/other-2.0.0.crate"),
        b"second reviewed original\n",
    )
    .unwrap();
    change_manifest(world, |manifest| {
        let mut artifact = manifest["artifacts"][0].clone();
        artifact["name"] = json!("other");
        artifact["version"] = json!("2.0.0");
        artifact["filename"] = json!("other-2.0.0.crate");
        artifact["sha256"] = json!(Sha256Digest::from_bytes(b"second reviewed original\n"));
        manifest["artifacts"] = json!([artifact]);
    });
}

#[when("the same review is published again before another reviewed batch")]
fn repeat_and_append(world: &World) {
    let before = storage_refs(world);
    let output = world.run(&["push", "--manifest", "reviewed.json"]);
    success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("up to date"));
    assert_eq!(storage_refs(world), before);
    append_review(world);
    ingest_and_publish(world);
}

#[then("the storage history is a two-commit linear audit trail")]
fn linear_history(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    let remote = world.temp.path().join("remote.git");
    assert_eq!(git(&remote, &["rev-list", "--count", "isolock/v1"]), b"2\n");
    assert_eq!(
        git(&remote, &["rev-list", "--min-parents=2", "isolock/v1"]),
        b""
    );
    assert_eq!(
        git(&remote, &["log", "-1", "--format=%s", "isolock/v1"]),
        b"chore(deps): add cargo/other==2.0.0\n"
    );
}

#[then("the first artifact remains byte-identical")]
fn first_unchanged(world: &World) {
    let path = format!(
        "isolock/v1:cas/{}",
        Sha256Digest::from_bytes(ORIGINAL).sharded_path()
    );
    assert_eq!(
        git(&world.temp.path().join("remote.git"), &["show", &path]),
        ORIGINAL
    );
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Repeated publication is idempotent and later batches append"
)]
fn idempotent_linear_publication(world: World) {
    let _ = world;
}

fn alter_storage(world: &World, path: &str, bytes: &[u8], executable: bool) {
    use std::os::unix::fs::PermissionsExt;
    let clone = tempfile::tempdir().unwrap();
    git(
        clone.path(),
        &[
            "clone",
            "--quiet",
            "--branch",
            "isolock/v1",
            world.temp.path().join("remote.git").to_str().unwrap(),
            ".",
        ],
    );
    git(clone.path(), &["config", "user.name", "Other publisher"]);
    git(
        clone.path(),
        &["config", "user.email", "other@example.invalid"],
    );
    fs::write(clone.path().join(path), bytes).unwrap();
    if executable {
        fs::set_permissions(clone.path().join(path), fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(clone.path(), &["add", "."]);
    git(
        clone.path(),
        &["commit", "--quiet", "-m", "test: changed remote snapshot"],
    );
    git(
        clone.path(),
        &["push", "--quiet", "origin", "HEAD:refs/heads/isolock/v1"],
    );
}

#[given("publication has {problem:string}")]
fn publication_problem(world: &World, problem: String) {
    if problem != "missing CAS artifact" {
        ingest(world);
        success(world.output.borrow().as_ref().unwrap());
    }
    let published = matches!(
        problem.as_str(),
        "altered storage marker"
            | "executable storage mode"
            | "malformed published plan"
            | "oversized remote metadata"
            | "changed review provenance"
            | "changed reviewed bytes"
            | "changed package variant"
    );
    if published {
        success(&world.run(&["push", "--manifest", "reviewed.json"]));
    }
    match problem.as_str() {
        "missing CAS artifact" => (),
        "corrupt CAS artifact" => fs::write(
            world.cas().path(&Sha256Digest::from_bytes(ORIGINAL)),
            b"corrupt cache",
        )
        .unwrap(),
        "code branch destination" => world
            .arguments
            .borrow_mut()
            .extend(["--storage-ref".into(), "main".into()]),
        "malformed epoch" => world
            .arguments
            .borrow_mut()
            .extend(["--storage-ref".into(), "isolock/v01".into()]),
        "unknown remote" => world
            .arguments
            .borrow_mut()
            .extend(["--remote".into(), "unknown".into()]),
        "credential remote" => world.arguments.borrow_mut().extend([
            "--remote".into(),
            "https://user:do-not-echo-this-secret@example.invalid/repo".into(),
        ]),
        "remote helper injection" => world
            .arguments
            .borrow_mut()
            .extend(["--remote".into(), "ext::untrusted-helper".into()]),
        "code tree on storage ref" => {
            git(
                world.temp.path(),
                &["push", "--quiet", "origin", "HEAD:refs/heads/isolock/v1"],
            );
        }
        "altered storage marker" => alter_storage(world, "isolock-storage.json", b"{}\n", false),
        "executable storage mode" => alter_storage(world, "isolock-storage.json", b"{}\n", true),
        "malformed published plan" => alter_storage(world, "plan.json", b"invalid JSON", false),
        "oversized remote metadata" => {
            alter_storage(world, "plan.json", &vec![b' '; 4 * 1024 * 1024 + 1], false)
        }
        "changed review provenance" => change_manifest(world, |m| {
            m["artifacts"][0]["source"] = json!("https://github.com/example/changed-provenance")
        }),
        "changed reviewed bytes" | "changed package variant" => {
            let filename = if problem == "changed package variant" {
                "demo-variant.crate"
            } else {
                "demo-1.0.0.crate"
            };
            fs::write(
                world.temp.path().join("originals").join(filename),
                b"different reviewed bytes",
            )
            .unwrap();
            change_manifest(world, |m| {
                m["artifacts"][0]["sha256"] =
                    json!(Sha256Digest::from_bytes(b"different reviewed bytes"));
                m["artifacts"][0]["filename"] = json!(filename);
            });
            ingest(world);
            success(world.output.borrow().as_ref().unwrap());
        }
        _ => panic!("unknown publication problem: {problem}"),
    }
    *world.remote_before.borrow_mut() = storage_refs(world);
    capture_checkout(world);
}

#[when("explicit publication is attempted")]
fn try_publish(world: &World) {
    let args = world.arguments.borrow();
    *world.output.borrow_mut() =
        Some(world.run(&args.iter().map(String::as_str).collect::<Vec<_>>()));
}

#[then("publication fails with {diagnostic:string} without changing remote references")]
fn rejected_publication(world: &World, diagnostic: String) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&diagnostic), "{stderr}");
    assert!(!stderr.contains("do-not-echo-this-secret"), "{stderr}");
    assert_eq!(storage_refs(world), *world.remote_before.borrow());
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Rejected publication never advances storage references"
)]
fn rejected_publication_is_non_mutating(world: World, problem: String, diagnostic: String) {
    let _ = (world, problem, diagnostic);
}

#[given("another publisher advances storage during publication")]
fn concurrent_publication(world: &World) {
    use std::os::unix::fs::PermissionsExt;
    ingest_and_publish(world);
    success(world.output.borrow().as_ref().unwrap());
    let root = world.temp.path();
    let remote = root.join("remote.git");
    let previous = git(&remote, &["rev-parse", "isolock/v1"]);
    let tree = git(&remote, &["rev-parse", "isolock/v1^{tree}"]);
    git(&remote, &["config", "user.name", "Concurrent publisher"]);
    git(&remote, &["config", "user.email", "other@example.invalid"]);
    let next = git(
        &remote,
        &[
            "commit-tree",
            std::str::from_utf8(&tree).unwrap().trim(),
            "-p",
            std::str::from_utf8(&previous).unwrap().trim(),
            "-m",
            "chore(deps): add concurrent review",
        ],
    );
    let hook = root.join(".git/hooks/pre-push");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\nset -eu\n# This is a foreign repository, not the private publisher Git directory.\nunset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE\ngit -C remote.git update-ref refs/heads/isolock/v1 {} {}\n",
            std::str::from_utf8(&next).unwrap().trim(),
            std::str::from_utf8(&previous).unwrap().trim()
        ),
    )
    .unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o755)).unwrap();
    *world.remote_before.borrow_mut() = next;
    append_review(world);
    ingest(world);
    success(world.output.borrow().as_ref().unwrap());
    capture_checkout(world);
}

#[then("the concurrent storage tip is preserved and retry succeeds linearly")]
fn no_force_push(world: &World) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("non-force publication"));
    let remote = world.temp.path().join("remote.git");
    assert_eq!(
        git(&remote, &["rev-parse", "isolock/v1"]),
        *world.remote_before.borrow()
    );
    fs::remove_file(world.temp.path().join(".git/hooks/pre-push")).unwrap();
    success(&world.run(&["push", "--manifest", "reviewed.json"]));
    assert_eq!(
        git(&remote, &["rev-parse", "isolock/v1^"]),
        *world.remote_before.borrow()
    );
    assert_eq!(git(&remote, &["rev-list", "--count", "isolock/v1"]), b"3\n");
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Concurrent publication is never force-pushed"
)]
fn concurrent_publish_is_not_force_pushed(world: World) {
    let _ = world;
}

#[given("a connected acquisition boundary returning {response:string}")]
fn acquisition_boundary(world: &World, response: String) {
    use std::os::unix::fs::PermissionsExt;
    let root = world.temp.path();
    fs::create_dir(root.join("tools")).unwrap();
    if response == "reserved basename" {
        change_manifest(world, |manifest| {
            manifest["artifacts"][0]["filename"] = json!("cas")
        });
    }
    let script = match response.as_str() {
        "reviewed bytes" | "reserved basename" => "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$@\" > \"$DOWNLOAD_CALLS\"\nwhile [ \"$#\" -gt 0 ]; do if [ \"$1\" = --output ]; then shift; output=$1; fi; shift; done\n/bin/cp \"$DOWNLOAD_SOURCE\" \"$output\"\n",
        "changed bytes" => "#!/bin/sh\nset -eu\nwhile [ \"$#\" -gt 0 ]; do if [ \"$1\" = --output ]; then shift; output=$1; fi; shift; done\nprintf changed > \"$output\"\n",
        "download fails" => "#!/bin/sh\necho do-not-echo-this-secret >&2\nexit 22\n",
        "missing curl" => return,
        _ => panic!("unexpected acquisition response: {response}"),
    };
    fs::write(root.join("tools/curl"), script).unwrap();
    fs::set_permissions(root.join("tools/curl"), fs::Permissions::from_mode(0o755)).unwrap();
}

#[when("explicitly connected ingestion is requested")]
fn connected_ingest(world: &World) {
    *world.output.borrow_mut() = Some(
        world
            .command(&["ingest", "--manifest", "reviewed.json", "--download"])
            .env("PATH", world.temp.path().join("tools"))
            .env(
                "DOWNLOAD_SOURCE",
                world.temp.path().join("originals/demo-1.0.0.crate"),
            )
            .env("DOWNLOAD_CALLS", world.temp.path().join("calls.txt"))
            .output()
            .unwrap(),
    );
}

#[then("acquisition has the reviewed outcome {outcome:string}")]
fn acquisition_result(world: &World, outcome: String) {
    if outcome == "verified" {
        verified_cas(world);
        let calls = fs::read_to_string(world.temp.path().join("calls.txt")).unwrap();
        assert!(calls.starts_with("--disable\n"), "{calls}");
        assert!(calls.contains("--globoff\n"), "{calls}");
        assert!(calls.contains("--proto\n=https\n"), "{calls}");
        assert!(calls.contains("--proto-redir\n=https\n"), "{calls}");
        assert!(
            calls.ends_with("https://github.com/example/reviewed/demo-1.0.0.crate\n"),
            "{calls}"
        );
    } else {
        rejected_ingestion(world, outcome);
    }
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Connected acquisition still verifies reviewed pins"
)]
fn explicit_download_uses_reviewed_pins(world: World, response: String, outcome: String) {
    let _ = (world, response, outcome);
}

#[when("the dispatch workflow receives {packages:string}")]
fn workflow_dispatch(world: &World, packages: String) {
    let workflow: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../../../.github/workflows/isolock.yml")).unwrap();
    let steps = workflow["jobs"]["ingest-dispatch"]["steps"]
        .as_sequence()
        .unwrap();
    let script = steps
        .iter()
        .find(|step| step["name"].as_str() == Some("Ingest and publish the reviewed selection"))
        .expect("workflow must invoke the implemented reviewed ingestion/publication path")["run"]
        .as_str()
        .unwrap();
    let mut command = Command::new("/bin/bash");
    command
        .env_clear()
        .current_dir(world.temp.path())
        .env("HOME", world.temp.path().join("empty-home"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "PATH",
            format!(
                "{}:{}",
                world.temp.path().join("tools").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env(
            "DOWNLOAD_SOURCE",
            world.temp.path().join("originals/demo-1.0.0.crate"),
        )
        .env("DOWNLOAD_CALLS", world.temp.path().join("calls.txt"))
        .env("ISOLOCK_BIN", env!("CARGO_BIN_EXE_isolock"))
        .env("ISOLOCK_INGEST_MANIFEST", "reviewed.json")
        .env("ISOLOCK_REGISTRY", "cargo")
        .env("ISOLOCK_PACKAGES", packages)
        .args(["-eu", "-c", script]);
    coverage_support::preserve_profile(&mut command);
    *world.output.borrow_mut() = Some(command.output().unwrap());
}

#[then("the workflow outcome is {outcome:string} without executing package text")]
fn workflow_result(world: &World, outcome: String) {
    assert!(!world.temp.path().join("marker").exists());
    assert!(!world.temp.path().join("main").exists());
    if outcome == "published" {
        published(world);
    } else {
        assert!(!world.output.borrow().as_ref().unwrap().status.success());
        assert!(storage_refs(world).is_empty());
        assert!(!world.temp.path().join("calls.txt").exists());
        assert!(world.cas().digests().unwrap().is_empty());
    }
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Workflow dispatch treats package input as literal reviewed selections"
)]
fn workflow_dispatch_is_literal_and_reviewed(world: World, packages: String, outcome: String) {
    let _ = (world, packages, outcome);
}

#[when("garbage collection runs here and in another ingesting workspace")]
fn shared_ingestion_gc(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    let first = world.run(&["gc", "--apply"]);
    success(&first);
    let report: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(report["roots"].as_array().unwrap().len(), 1);
    assert!(report["roots"][0]
        .as_str()
        .unwrap()
        .starts_with("ingestion/"));
    let other = world.temp.path().join("other");
    fs::create_dir(&other).unwrap();
    let mut review: serde_json::Value =
        serde_json::from_slice(&fs::read(world.manifest()).unwrap()).unwrap();
    review["artifacts"][0]["name"] = json!("other");
    review["artifacts"][0]["sha256"] = json!(Sha256Digest::from_bytes(b"other reviewed bytes"));
    fs::write(
        other.join("reviewed.json"),
        serde_json::to_vec(&review).unwrap(),
    )
    .unwrap();
    fs::write(other.join("demo-1.0.0.crate"), b"other reviewed bytes").unwrap();
    success(
        &world
            .command(&[
                "ingest",
                "--manifest",
                "reviewed.json",
                "--cas-dir",
                "../custom-cas",
            ])
            .current_dir(&other)
            .output()
            .unwrap(),
    );
    // Disappearing workspaces must not silently revoke their durable CAS registration.
    fs::remove_dir_all(world.temp.path().join(".isolock")).unwrap();
    *world.output.borrow_mut() = Some(
        world
            .command(&["gc", "--apply", "--cas-dir", "../custom-cas"])
            .current_dir(other)
            .output()
            .unwrap(),
    );
}

#[then("both workspaces retain their reviewed originals without publication")]
fn retained_reviews(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    for bytes in [ORIGINAL, b"other reviewed bytes".as_slice()] {
        assert!(world
            .cas()
            .verified(&Sha256Digest::from_bytes(bytes))
            .unwrap());
    }
    assert_eq!(world.cas().digests().unwrap().len(), 2);
    no_publication(world);
}

fn ingestion_object(world: &World) -> PathBuf {
    fs::read_dir(world.temp.path().join(".isolock/ingestion/objects"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
}

#[given("an ingested review object has {problem:string}")]
fn corrupt_ingestion_root(world: &World, problem: String) {
    ingest(world);
    success(world.output.borrow().as_ref().unwrap());
    let path = ingestion_object(world);
    match problem.as_str() {
        "corrupt JSON" => fs::write(path, b"{}").unwrap(),
        "noncanonical object" => {
            let name = path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_ascii_uppercase();
            fs::rename(&path, path.with_file_name(format!("{name}.json"))).unwrap();
        }
        "misnamed object" => {
            fs::rename(
                &path,
                path.with_file_name(format!("{}.json", "0".repeat(64))),
            )
            .unwrap();
        }
        "unexpected filename" => {
            fs::rename(&path, path.with_file_name("not-an-object.txt")).unwrap();
        }
        "symlink object" => {
            let saved = world.temp.path().join("saved-review.json");
            fs::rename(&path, &saved).unwrap();
            std::os::unix::fs::symlink(saved, path).unwrap();
        }
        _ => panic!("unknown ingestion root problem: {problem}"),
    }
}

#[when("garbage collection is explicitly applied")]
fn collect_reviews(world: &World) {
    *world.output.borrow_mut() = Some(world.run(&["gc", "--apply"]));
}

#[then("collection rejects the damaged ingestion root and retains the bytes")]
fn rejects_damaged_root(world: &World) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("ingestion"),
        "{output:?}"
    );
    assert!(world
        .cas()
        .verified(&Sha256Digest::from_bytes(ORIGINAL))
        .unwrap());
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Ingested reviews are durable shared CAS roots before publication"
)]
fn ingested_review_roots_survive_gc(world: World) {
    let _ = world;
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "Damaged ingestion roots stop garbage collection"
)]
fn damaged_review_roots_fail_closed(world: World, problem: String) {
    let _ = (world, problem);
}

#[when("the workflow audits storage after producer originals and caches are removed")]
fn audit_published_storage(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    fs::remove_dir_all(world.temp.path().join("originals")).unwrap();
    fs::remove_dir_all(world.cas().root()).unwrap();
    git(
        world.temp.path(),
        &[
            "fetch",
            "--quiet",
            "origin",
            "refs/heads/isolock/v1:refs/remotes/origin/isolock/v1",
        ],
    );
    let workflow: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../../../.github/workflows/isolock.yml")).unwrap();
    let script = workflow["jobs"]["plan-guard"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|step| step["name"].as_str() == Some("Run Isolock Hermeticity Audit"))
        .unwrap()["run"]
        .as_str()
        .unwrap();
    let temporary = world.temp.path().join(".isolock/tmp");
    fs::create_dir_all(&temporary).unwrap();
    let mut command = Command::new("/bin/bash");
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", world.temp.path().join("empty-home"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("ISOLOCK_BIN", env!("CARGO_BIN_EXE_isolock"))
        .env("ISOLOCK_SNAPSHOT_REF", "refs/remotes/origin/isolock/v1")
        .env("RUNNER_TEMP", temporary)
        .current_dir(world.temp.path())
        .args(["-eu", "-c", script]);
    coverage_support::preserve_profile(&mut command);
    *world.output.borrow_mut() = Some(command.output().unwrap());
}

#[then("the workflow guard verifies the published airlock snapshot")]
fn audited_published_snapshot(world: &World) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    success(output);
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("ALL DEPENDENCIES SATISFIED"),
        "{output:?}"
    );
    assert!(!world.cas().root().exists());
}

#[scenario(
    path = "../../backlog/features/reviewed-ingestion.feature",
    name = "The PR guard verifies the published snapshot rather than producer caches"
)]
fn published_snapshot_guard_is_cold(world: World) {
    let _ = world;
}
