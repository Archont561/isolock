//! Epoch contracts run through the public CLI and real disposable Git remotes.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid fixtures must fail the test.

mod coverage_support;

use isolock_cas::Sha256Digest;
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use serde_json::json;
use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

struct World {
    temp: TempDir,
    output: RefCell<Option<Output>>,
    before: RefCell<Vec<Vec<u8>>>,
    source: RefCell<String>,
    expected_refs: RefCell<Vec<u8>>,
}

#[fixture]
fn world() -> World {
    World {
        temp: tempfile::tempdir().unwrap(),
        output: RefCell::new(None),
        before: RefCell::new(Vec::new()),
        source: RefCell::new(String::new()),
        expected_refs: RefCell::new(Vec::new()),
    }
}

fn success(output: &Output) {
    assert!(output.status.success(), "{output:?}");
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
    fn remote(&self, arguments: &[&str]) -> Vec<u8> {
        git(&self.temp.path().join("remote.git"), arguments)
    }
    fn review(&self, name: &str, bytes: &[u8]) {
        fs::write(
            self.temp.path().join(format!("originals/{name}.crate")),
            bytes,
        )
        .unwrap();
        fs::write(
            self.temp.path().join(format!("{name}.json")),
            serde_json::to_vec_pretty(&json!({
                "schema-version":1,"artifacts":[{"registry":"cargo","name":name,"version":"1.0.0",
                "filename":format!("{name}.crate"),"sha256":Sha256Digest::from_bytes(bytes),
                "source":format!("https://github.com/example/reviewed/{name}.crate")}]
            }))
            .unwrap(),
        )
        .unwrap();
        success(&self.run(&[
            "ingest",
            "--manifest",
            &format!("{name}.json"),
            "--artifacts",
            "originals",
        ]));
    }
    fn capture(&self) -> Vec<Vec<u8>> {
        vec![
            git(self.temp.path(), &["rev-parse", "HEAD"]),
            git(self.temp.path(), &["status", "--porcelain=v1", "-z"]),
            fs::read(self.temp.path().join(".git/index")).unwrap(),
            fs::read(self.temp.path().join("isolock.toml")).unwrap(),
            fs::read(self.temp.path().join("code.txt")).unwrap(),
            fs::read(self.temp.path().join(".git/config")).unwrap(),
            fs::read(self.temp.path().join(".git/FETCH_HEAD")).unwrap_or_default(),
            fs::read(self.temp.path().join(".git/shallow")).unwrap_or_default(),
            git(
                self.temp.path(),
                &["for-each-ref", "--format=%(refname) %(objectname)"],
            ),
        ]
    }
}

fn git(root: &Path, arguments: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", root.join("empty-home"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .unwrap();
    success(&output);
    output.stdout
}

#[given("reviewed artifacts and a dirty producer checkout on a bounded storage epoch")]
fn producer(world: &World) {
    let root = world.temp.path();
    fs::create_dir(root.join("originals")).unwrap();
    fs::write(root.join("isolock.toml"), "[storage]\ncas-dir='cas'\n").unwrap();
    fs::write(
        root.join(".gitignore"),
        "cas/\noriginals/\n.isolock/\nremote.git/\nconsumer/\n",
    )
    .unwrap();
    fs::write(root.join("code.txt"), "original code\n").unwrap();
    git(root, &["init", "--quiet", "--initial-branch=main"]);
    git(root, &["config", "user.name", "Reviewed epoch publisher"]);
    git(root, &["config", "user.email", "epochs@example.invalid"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "test: producer code"]);
    git(root, &["init", "--quiet", "--bare", "remote.git"]);
    git(root, &["remote", "add", "origin", "remote.git"]);
    world.review("old", &vec![b'a'; 4096]);
    success(&world.run(&["push", "--manifest", "old.json"]));
    *world.source.borrow_mut() = String::from_utf8(world.remote(&["rev-parse", "isolock/v1"]))
        .unwrap()
        .trim()
        .into();
    world.review("new", &vec![b'b'; 4096]);
    fs::write(root.join("code.txt"), "staged code\n").unwrap();
    git(root, &["add", "code.txt"]);
    fs::write(root.join("code.txt"), "unstaged code\n").unwrap();
}

#[given("a reviewed rollover policy pinned to that storage tip")]
fn policy(world: &World) {
    fs::write(
        world.temp.path().join("epoch-policy.json"),
        serde_json::to_vec_pretty(&json!({
            "schema-version":1,"source-ref":"isolock/v1","source-commit":*world.source.borrow(),
            "next-ref":"isolock/v2","max-live-bytes":8192,"max-history-bytes":8192,
            "retain-batches":[],"retain-manifests":[]
        }))
        .unwrap(),
    )
    .unwrap();
    *world.before.borrow_mut() = world.capture();
}

#[when("another reviewed batch is published with that policy")]
fn publish(world: &World) {
    *world.output.borrow_mut() = Some(world.run(&[
        "push",
        "--manifest",
        "new.json",
        "--epoch-policy",
        "epoch-policy.json",
    ]));
}

#[then("a fresh orphan epoch contains the new reviewed batch within budget")]
fn fresh_epoch(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    assert_eq!(world.remote(&["rev-list", "--count", "isolock/v2"]), b"1\n");
    let new = format!(
        "isolock/v2:cas/{}",
        Sha256Digest::from_bytes(&vec![b'b'; 4096]).sharded_path()
    );
    assert_eq!(world.remote(&["show", &new]), vec![b'b'; 4096]);
    let paths =
        String::from_utf8(world.remote(&["ls-tree", "-r", "--name-only", "isolock/v2"])).unwrap();
    assert!(!paths.contains(Sha256Digest::from_bytes(&vec![b'a'; 4096]).as_str()));
    assert!(paths.lines().any(|path| path == "epoch.json"));
    let plan: isolock_plan::AirlockPlan =
        serde_json::from_slice(&world.remote(&["show", "isolock/v2:plan.json"])).unwrap();
    assert_eq!(plan.artifacts().len(), 1);
    assert_eq!(plan.artifacts()[0].name, "new");
    let live: u64 = String::from_utf8(world.remote(&["ls-tree", "-rl", "isolock/v2"]))
        .unwrap()
        .lines()
        .map(|entry| {
            entry
                .split_whitespace()
                .nth(3)
                .unwrap()
                .parse::<u64>()
                .unwrap()
        })
        .sum();
    assert!(live <= 8192, "live bytes: {live}");
}

#[then("the old epoch is sealed without changing its snapshot bytes")]
fn sealed_epoch(world: &World) {
    assert_eq!(
        String::from_utf8(world.remote(&["rev-parse", "isolock/v1^"]))
            .unwrap()
            .trim(),
        *world.source.borrow()
    );
    assert_eq!(
        world.remote(&["rev-parse", "isolock/v1^{tree}"]),
        world.remote(&["rev-parse", &format!("{}^{{tree}}", world.source.borrow())])
    );
    assert!(
        String::from_utf8(world.remote(&["log", "-1", "--format=%B", "isolock/v1"]))
            .unwrap()
            .contains("Isolock-Epoch-Seal:")
    );
    let output = world.run(&["push", "--manifest", "new.json"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("sealed"),
        "{output:?}"
    );
}

#[then("the producer checkout and project configuration remain unchanged")]
fn unchanged(world: &World) {
    assert_eq!(world.capture(), *world.before.borrow());
}

#[then("a cold consumer verifies the new epoch without producer originals or caches")]
fn cold_consumer(world: &World) {
    fs::remove_dir_all(world.temp.path().join("originals")).unwrap();
    fs::remove_dir_all(world.temp.path().join("cas")).unwrap();
    git(
        world.temp.path(),
        &[
            "clone",
            "--quiet",
            "--no-local",
            "--single-branch",
            "--branch",
            "isolock/v2",
            "remote.git",
            "consumer",
        ],
    );
    let output = world
        .command(&["plan", "--ci", "--cas-dir", "cas"])
        .env("ISOLOCK_PLAN", "plan.json")
        .current_dir(world.temp.path().join("consumer"))
        .output()
        .unwrap();
    success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("ALL DEPENDENCIES SATISFIED"));
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Budget overflow rolls into a reviewed orphan epoch without changing code"
)]
fn reviewed_budget_rollover(world: World) {
    let _ = world;
}

fn refs(world: &World) -> Vec<u8> {
    world.remote(&["for-each-ref", "--format=%(refname) %(objectname)"])
}

#[when("an explicit publisher appends to the successor")]
fn append_successor(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    world.review("later", b"a later reviewed original\n");
    success(&world.run(&[
        "push",
        "--manifest",
        "later.json",
        "--storage-ref",
        "isolock/v2",
    ]));
}

#[then("an exact rollover retry leaves all remote references unchanged")]
fn exact_retry(world: &World) {
    let before = refs(world);
    publish(world);
    success(world.output.borrow().as_ref().unwrap());
    assert!(
        String::from_utf8_lossy(&world.output.borrow().as_ref().unwrap().stdout)
            .contains("up to date")
    );
    assert_eq!(refs(world), before);
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Accepted rollover retries are idempotent after successor appends"
)]
fn accepted_retry_after_append(world: World) {
    let _ = world;
}

fn executable(path: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[given("the epoch boundary encounters {problem:string}")]
fn atomic_boundary(world: &World, problem: String) {
    *world.expected_refs.borrow_mut() = refs(world);
    match problem.as_str() {
        "a source race" | "a successor race" => {
            world.remote(&["config", "user.name", "Concurrent publisher"]);
            world.remote(&["config", "user.email", "other@example.invalid"]);
            let tree =
                String::from_utf8(world.remote(&["rev-parse", "isolock/v1^{tree}"])).unwrap();
            let mut args = vec!["commit-tree", tree.trim()];
            let source = world.source.borrow();
            if problem == "a source race" {
                args.extend(["-p", source.as_str()]);
            }
            args.extend(["-m", "test: competing publisher"]);
            let commit = String::from_utf8(world.remote(&args)).unwrap();
            let reference = if problem == "a source race" {
                "isolock/v1"
            } else {
                "isolock/v2"
            };
            executable(&world.temp.path().join(".git/hooks/pre-push"), &format!("#!/bin/sh\nset -eu\nunset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE\ngit -C remote.git update-ref refs/heads/{reference} {}\n", commit.trim()));
            *world.expected_refs.borrow_mut() = if problem == "a source race" {
                format!("refs/heads/isolock/v1 {}\n", commit.trim()).into_bytes()
            } else {
                format!(
                    "refs/heads/isolock/v1 {}\nrefs/heads/isolock/v2 {}\n",
                    source.as_str(),
                    commit.trim()
                )
                .into_bytes()
            };
        }
        "a rejected target" => executable(
            &world.temp.path().join("remote.git/hooks/update"),
            "#!/bin/sh\n[ \"$1\" != refs/heads/isolock/v2 ]\n",
        ),
        "no atomic support" => {
            world.remote(&["config", "receive.advertiseAtomic", "false"]);
        }
        _ => panic!("unknown boundary {problem}"),
    }
}

#[then("epoch publication fails with {diagnostic:string} and preserves concurrent references")]
fn rejected_epoch(world: &World, diagnostic: String) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("do-not-echo-this-secret"));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(&diagnostic),
        "{output:?}"
    );
    assert_eq!(refs(world), *world.expected_refs.borrow());
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Competing publishers and partial failures never publish half an epoch"
)]
fn atomic_failure_is_non_mutating(world: World, problem: String) {
    let _ = (world, problem);
}

fn edit_policy(world: &World, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = world.temp.path().join("epoch-policy.json");
    let mut value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut value);
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn edit_snapshot(world: &World, edit: impl FnOnce(&Path)) {
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
    git(
        clone.path(),
        &["config", "user.name", "Migration boundary fixture"],
    );
    git(
        clone.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    edit(clone.path());
    git(clone.path(), &["add", "-A"]);
    git(
        clone.path(),
        &["commit", "--quiet", "-m", "test: storage format boundary"],
    );
    git(
        clone.path(),
        &["push", "--quiet", "origin", "HEAD:refs/heads/isolock/v1"],
    );
    *world.source.borrow_mut() = String::from_utf8(world.remote(&["rev-parse", "isolock/v1"]))
        .unwrap()
        .trim()
        .into();
    edit_policy(world, |policy| {
        policy["source-commit"] = json!(*world.source.borrow())
    });
}

fn old_batch(world: &World) -> String {
    String::from_utf8(world.remote(&["ls-tree", "--name-only", "isolock/v1:ingestion/objects"]))
        .unwrap()
        .trim()
        .strip_suffix(".json")
        .unwrap()
        .into()
}

#[given("epoch migration has {problem:string}")]
fn migration_problem(world: &World, problem: String) {
    let missing = Sha256Digest::from_bytes(b"an unpublished root");
    let old = old_batch(world);
    match problem.as_str() {
        "missing policy" => fs::remove_file(world.temp.path().join("epoch-policy.json")).unwrap(),
        "unknown policy field" => {
            edit_policy(world, |p| p["secret"] = json!("do-not-echo-this-secret"))
        }
        "unsupported policy schema" => edit_policy(world, |p| p["schema-version"] = json!(2)),
        "oversized policy" => fs::write(
            world.temp.path().join("epoch-policy.json"),
            vec![b' '; 4 * 1024 * 1024 + 1],
        )
        .unwrap(),
        "invalid source commit" => edit_policy(world, |p| p["source-commit"] = json!("bad-commit")),
        "stale source commit" => edit_policy(world, |p| p["source-commit"] = json!("0".repeat(40))),
        "different source ref" => edit_policy(world, |p| {
            p["source-ref"] = json!("isolock/v2");
            p["next-ref"] = json!("isolock/v3");
        }),
        "code source ref" => edit_policy(world, |p| p["source-ref"] = json!("main")),
        "leading zero generation" => edit_policy(world, |p| p["source-ref"] = json!("isolock/v01")),
        "generation overflow" => edit_policy(world, |p| {
            p["source-ref"] = json!("isolock/v18446744073709551615")
        }),
        "skipped successor" => edit_policy(world, |p| p["next-ref"] = json!("isolock/v3")),
        "zero live budget" => edit_policy(world, |p| p["max-live-bytes"] = json!(0)),
        "zero history budget" => edit_policy(world, |p| p["max-history-bytes"] = json!(0)),
        "oversized live budget" => edit_policy(world, |p| {
            p["max-live-bytes"] = json!(1500 * 1024 * 1024_u64 + 1)
        }),
        "oversized history budget" => edit_policy(world, |p| {
            p["max-history-bytes"] = json!(1500 * 1024 * 1024_u64 + 1)
        }),
        "duplicate batch root" => edit_policy(world, |p| p["retain-batches"] = json!([old, old])),
        "duplicate manifest root" => {
            edit_policy(world, |p| p["retain-manifests"] = json!([missing, missing]))
        }
        "malformed retention root" => {
            edit_policy(world, |p| p["retain-batches"] = json!(["not-a-digest"]))
        }
        "missing retained batch" => edit_policy(world, |p| p["retain-batches"] = json!([missing])),
        "missing retained manifest" => {
            edit_policy(world, |p| p["retain-manifests"] = json!([missing]))
        }
        "oversized retained closure" => edit_policy(world, |p| p["retain-batches"] = json!([old])),
        "oversized incoming closure" => edit_policy(world, |p| {
            p["max-live-bytes"] = json!(1024);
            p["max-history-bytes"] = json!(1024);
        }),
        "existing successor" => {
            world.remote(&[
                "update-ref",
                "refs/heads/isolock/v2",
                world.source.borrow().as_str(),
            ]);
        }
        "unmarked snapshot" => edit_snapshot(world, |root| {
            fs::remove_file(root.join("isolock-storage.json")).unwrap()
        }),
        "legacy shards" => edit_snapshot(world, |root| {
            fs::create_dir_all(root.join("cas/shards")).unwrap();
            fs::write(root.join("cas/shards/0001.part"), b"legacy shard").unwrap();
        }),
        "unsafe symlink" => edit_snapshot(world, |root| {
            fs::remove_file(root.join("isolock-storage.json")).unwrap();
            std::os::unix::fs::symlink("plan.json", root.join("isolock-storage.json")).unwrap();
        }),
        "unsafe executable" => edit_snapshot(world, |root| {
            executable(&root.join("isolock-storage.json"), "{}")
        }),
        "shallow storage history" => {
            git(
                world.temp.path(),
                &[
                    "clone",
                    "--quiet",
                    "--bare",
                    "--depth=1",
                    "--branch",
                    "isolock/v1",
                    &format!("file://{}", world.temp.path().join("remote.git").display()),
                    "shallow.git",
                ],
            );
            git(
                world.temp.path(),
                &["remote", "set-url", "origin", "shallow.git"],
            );
        }
        "invalid epoch record" => edit_snapshot(world, |root| {
            fs::write(root.join("epoch.json"), b"{}").unwrap()
        }),
        "corrupt retained original"
        | "missing retained original"
        | "missing provenance record"
        | "altered provenance"
        | "altered review" => {
            edit_snapshot(world, |root| match problem.as_str() {
                "corrupt retained original" => fs::write(
                    root.join(format!(
                        "cas/{}",
                        Sha256Digest::from_bytes(&vec![b'a'; 4096]).sharded_path()
                    )),
                    b"wrong original",
                )
                .unwrap(),
                "missing retained original" => fs::remove_dir_all(root.join("cas")).unwrap(),
                "missing provenance record" => {
                    fs::remove_dir_all(root.join("ingestion/records")).unwrap()
                }
                "altered provenance" => {
                    let path = fs::read_dir(root.join("ingestion/records"))
                        .unwrap()
                        .next()
                        .unwrap()
                        .unwrap()
                        .path();
                    let mut artifact: serde_json::Value =
                        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                    artifact["source"] = json!("https://github.com/example/changed-provenance");
                    fs::write(path, serde_json::to_vec(&artifact).unwrap()).unwrap();
                }
                "altered review" => {
                    let path = root.join(format!("ingestion/objects/{old}.json"));
                    let mut batch: serde_json::Value =
                        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                    batch["artifacts"][0]["source"] =
                        json!("https://github.com/example/changed-review");
                    fs::write(path, serde_json::to_vec(&batch).unwrap()).unwrap();
                }
                _ => unreachable!(),
            });
            edit_policy(world, |p| p["retain-batches"] = json!([old]));
        }
        _ => panic!("unknown migration problem {problem}"),
    }
    *world.expected_refs.borrow_mut() = refs(world);
    *world.before.borrow_mut() = world.capture();
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Invalid review and migration boundaries fail closed"
)]
fn invalid_migration_is_non_mutating(world: World, problem: String, diagnostic: String) {
    let _ = (world, problem, diagnostic);
}

fn live_bytes(world: &World, reference: &str) -> u64 {
    String::from_utf8(world.remote(&["ls-tree", "-rl", reference]))
        .unwrap()
        .lines()
        .map(|line| {
            line.split_whitespace()
                .nth(3)
                .unwrap()
                .parse::<u64>()
                .unwrap()
        })
        .sum()
}

fn history_bytes(world: &World, reference: &str) -> u64 {
    let objects =
        String::from_utf8(world.remote(&["rev-list", "--objects", "--no-object-names", reference]))
            .unwrap();
    objects
        .lines()
        .filter(|object| world.remote(&["cat-file", "-t", object]) == b"blob\n")
        .map(|object| {
            String::from_utf8(world.remote(&["cat-file", "-s", object]))
                .unwrap()
                .trim()
                .parse::<u64>()
                .unwrap()
        })
        .sum()
}

#[given("the reviewed budgets allow the complete candidate")]
fn generous_budgets(world: &World) {
    edit_policy(world, |p| {
        p["max-live-bytes"] = json!(32768);
        p["max-history-bytes"] = json!(32768);
    });
}

#[then("publication appends to the source without creating or sealing an epoch")]
fn linear_under_budget(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    assert_eq!(
        String::from_utf8(world.remote(&["rev-parse", "isolock/v1^"]))
            .unwrap()
            .trim(),
        *world.source.borrow()
    );
    assert_eq!(world.remote(&["rev-list", "--count", "isolock/v1"]), b"2\n");
    assert!(
        !String::from_utf8_lossy(&world.remote(&["log", "--format=%B", "isolock/v1"]))
            .contains("Isolock-Epoch-Seal:")
    );
    assert!(!String::from_utf8_lossy(&refs(world)).contains("isolock/v2"));
}

#[given("removed historical blobs exceed the history budget with {retention:string}")]
fn historical_pressure(world: &World, retention: String) {
    let original = world.remote(&["show", "isolock/v1:plan.json"]);
    let mut inflated = original.clone();
    inflated.extend(vec![b' '; 32768]);
    edit_snapshot(world, |root| {
        fs::write(root.join("plan.json"), inflated).unwrap()
    });
    edit_snapshot(world, |root| {
        fs::write(root.join("plan.json"), original).unwrap()
    });
    assert!(history_bytes(world, "isolock/v1") > 32768);
    assert!(live_bytes(world, "isolock/v1") < 8192);
    let batch = old_batch(world);
    edit_policy(world, |p| {
        p["max-live-bytes"] = json!(32768);
        p["max-history-bytes"] = json!(16384);
        if retention == "retain the old batch" {
            p["retain-batches"] = json!([batch]);
        }
    });
    *world.before.borrow_mut() = world.capture();
}

#[then("the successor respects {retention:string} while the archived history remains available")]
fn history_budget_rollover(world: &World, retention: String) {
    success(world.output.borrow().as_ref().unwrap());
    assert_eq!(world.remote(&["rev-list", "--count", "isolock/v1"]), b"4\n");
    assert_eq!(world.remote(&["rev-list", "--count", "isolock/v2"]), b"1\n");
    assert!(live_bytes(world, "isolock/v2") <= 32768);
    assert!(history_bytes(world, "isolock/v2") <= 16384);
    let plan: isolock_plan::AirlockPlan =
        serde_json::from_slice(&world.remote(&["show", "isolock/v2:plan.json"])).unwrap();
    let names: std::collections::BTreeSet<_> = plan
        .artifacts()
        .iter()
        .map(|artifact| artifact.name.as_str())
        .collect();
    if retention == "retain the old batch" {
        assert_eq!(names, std::collections::BTreeSet::from(["new", "old"]));
    } else {
        assert_eq!(names, std::collections::BTreeSet::from(["new"]));
    }
    let old_path = format!(
        "cas/{}",
        Sha256Digest::from_bytes(&vec![b'a'; 4096]).sharded_path()
    );
    assert_eq!(
        world.remote(&["show", &format!("isolock/v1:{old_path}")]),
        vec![b'a'; 4096]
    );
    assert!(history_bytes(world, "isolock/v1") > 32768);
}

#[when("an existing batch is retried at exact live and history budgets")]
fn budget_equality(world: &World) {
    success(&world.run(&["push", "--manifest", "new.json"]));
    *world.source.borrow_mut() = String::from_utf8(world.remote(&["rev-parse", "isolock/v1"]))
        .unwrap()
        .trim()
        .into();
    policy(world);
    let live = live_bytes(world, "isolock/v1");
    let history = history_bytes(world, "isolock/v1");
    assert!(history > live, "the superseded plan adds historical bytes");
    edit_policy(world, |p| {
        p["max-live-bytes"] = json!(live);
        p["max-history-bytes"] = json!(history);
    });
    *world.expected_refs.borrow_mut() = refs(world);
    publish(world);
}

#[then("the original references are unchanged without an orphan successor")]
fn exact_budget_noop(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    assert!(
        String::from_utf8_lossy(&world.output.borrow().as_ref().unwrap().stdout)
            .contains("up to date")
    );
    assert_eq!(refs(world), *world.expected_refs.borrow());
    assert!(!String::from_utf8_lossy(&refs(world)).contains("isolock/v2"));
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Batches append linearly while both reviewed budgets fit"
)]
fn append_within_both_budgets(world: World) {
    let _ = world;
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Cumulative history triggers rollover even when the live tree fits"
)]
fn history_not_live_triggers_rollover(world: World, retention: String) {
    let _ = (world, retention);
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Exact budgets count repeated historical blobs only once"
)]
fn exact_budget_counts_each_blob_once(world: World) {
    let _ = world;
}

#[when("producer originals and CAS disappear")]
fn remove_producer_payloads(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    fs::remove_dir_all(world.temp.path().join("originals")).unwrap();
    fs::remove_dir_all(world.temp.path().join("cas")).unwrap();
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "An accepted rollover retry needs no producer cache"
)]
fn accepted_retry_without_cas(world: World) {
    let _ = world;
}

fn replace_seal(world: &World, message: &str) {
    world.remote(&["config", "user.name", "Changed history fixture"]);
    world.remote(&["config", "user.email", "changed@example.invalid"]);
    let tree = String::from_utf8(
        world.remote(&["rev-parse", &format!("{}^{{tree}}", world.source.borrow())]),
    )
    .unwrap();
    let next = String::from_utf8(world.remote(&[
        "commit-tree",
        tree.trim(),
        "-p",
        world.source.borrow().as_str(),
        "-m",
        message,
    ]))
    .unwrap();
    world.remote(&["update-ref", "refs/heads/isolock/v1", next.trim()]);
}

fn edit_seal(world: &World, edit: impl FnOnce(&mut serde_json::Value)) {
    let message =
        String::from_utf8(world.remote(&["log", "-1", "--format=%B", "isolock/v1"])).unwrap();
    let line = message
        .lines()
        .find_map(|line| line.strip_prefix("Isolock-Epoch-Seal:"))
        .unwrap();
    let mut seal = serde_json::from_str(line).unwrap();
    edit(&mut seal);
    replace_seal(
        world,
        &format!(
            "test: changed epoch seal\n\nIsolock-Epoch-Seal: {}",
            serde_json::to_string(&seal).unwrap()
        ),
    );
}

fn edit_accepted_target(world: &World, orphan: bool, edit: impl FnOnce(&Path)) {
    let clone = tempfile::tempdir().unwrap();
    git(
        clone.path(),
        &[
            "clone",
            "--quiet",
            "--branch",
            "isolock/v2",
            world.temp.path().join("remote.git").to_str().unwrap(),
            ".",
        ],
    );
    git(
        clone.path(),
        &["config", "user.name", "Changed target fixture"],
    );
    git(
        clone.path(),
        &["config", "user.email", "changed@example.invalid"],
    );
    edit(clone.path());
    git(clone.path(), &["add", "-A"]);
    git(
        clone.path(),
        &[
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "test: changed accepted target",
        ],
    );
    world.remote(&["fetch", "--quiet", clone.path().to_str().unwrap(), "HEAD"]);
    world.remote(&["config", "user.name", "Changed target fixture"]);
    world.remote(&["config", "user.email", "changed@example.invalid"]);
    let commit = if orphan {
        let tree = String::from_utf8(world.remote(&["rev-parse", "FETCH_HEAD^{tree}"])).unwrap();
        world.remote(&[
            "commit-tree",
            tree.trim(),
            "-m",
            "test: changed orphan root",
        ])
    } else {
        world.remote(&["rev-parse", "FETCH_HEAD"])
    };
    let commit = String::from_utf8(commit).unwrap();
    world.remote(&["update-ref", "refs/heads/isolock/v2", commit.trim()]);
    edit_seal(world, |seal| seal["target-commit"] = json!(commit.trim()));
}

#[given("an accepted rollover has {problem:string}")]
fn accepted_problem(world: &World, problem: String) {
    publish(world);
    success(world.output.borrow().as_ref().unwrap());
    match problem.as_str() {
        "a different reviewed policy" => edit_policy(world, |p| p["max-live-bytes"] = json!(16384)),
        "a different reviewed batch" => world.review("new", b"a different reviewed batch"),
        "a missing successor" => {
            world.remote(&["update-ref", "-d", "refs/heads/isolock/v2"]);
        }
        "a replaced successor" => {
            world.remote(&[
                "update-ref",
                "refs/heads/isolock/v2",
                world.source.borrow().as_str(),
            ]);
        }
        "an append after the source seal" => {
            world.remote(&["config", "user.name", "External publisher"]);
            world.remote(&["config", "user.email", "external@example.invalid"]);
            let tree =
                String::from_utf8(world.remote(&["rev-parse", "isolock/v1^{tree}"])).unwrap();
            let parent = String::from_utf8(world.remote(&["rev-parse", "isolock/v1"])).unwrap();
            let next = String::from_utf8(world.remote(&[
                "commit-tree",
                tree.trim(),
                "-p",
                parent.trim(),
                "-m",
                "test: external append after seal",
            ]))
            .unwrap();
            world.remote(&["update-ref", "refs/heads/isolock/v1", next.trim()]);
        }
        "a malformed source seal" => {
            replace_seal(world, "test: invalid seal\n\nIsolock-Epoch-Seal: not-json")
        }
        "an unsupported source seal" => edit_seal(world, |seal| seal["schema-version"] = json!(2)),
        "two source seal trailers" => {
            let mut message =
                String::from_utf8(world.remote(&["log", "-1", "--format=%B", "isolock/v1"]))
                    .unwrap();
            message.push_str("\nIsolock-Epoch-Seal: {}\n");
            replace_seal(world, &message);
        }
        "a non-orphan accepted target" => edit_accepted_target(world, false, |_| ()),
        "an invalid accepted record" => edit_accepted_target(world, true, |root| {
            fs::write(root.join("epoch.json"), b"{}").unwrap()
        }),
        "a changed accepted record" => edit_accepted_target(world, true, |root| {
            let path = root.join("epoch.json");
            let mut record: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            record["schema-version"] = json!(2);
            fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
        }),
        "a changed accepted marker" => edit_accepted_target(world, true, |root| {
            fs::write(root.join("isolock-storage.json"), b"{}").unwrap()
        }),
        "missing accepted originals" => edit_accepted_target(world, true, |root| {
            fs::remove_dir_all(root.join("cas")).unwrap()
        }),
        "a missing accepted review" => edit_accepted_target(world, true, |root| {
            fs::remove_dir_all(root.join("ingestion/objects")).unwrap()
        }),
        "an over-budget accepted target" => edit_accepted_target(world, true, |root| {
            let path = root.join("plan.json");
            let mut bytes = fs::read(&path).unwrap();
            bytes.extend(vec![b' '; 32768]);
            fs::write(path, bytes).unwrap();
        }),
        _ => panic!("unknown accepted history problem {problem}"),
    }
    *world.expected_refs.borrow_mut() = refs(world);
    *world.before.borrow_mut() = world.capture();
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "An accepted rollover cannot hide changed or incomplete remote history"
)]
fn changed_accepted_history_fails_closed(world: World, problem: String, diagnostic: String) {
    let _ = (world, problem, diagnostic);
}

#[given("the receiver disconnects after accepting the atomic references")]
fn disconnect_after_accept(world: &World) {
    // A real receive-pack process dies AFTER its atomic reference transaction.
    // This is not a fake Git response: both refs genuinely exist despite exit 1.
    executable(
        &world.temp.path().join("remote.git/hooks/post-receive"),
        "#!/bin/sh\nkill -TERM \"$PPID\"\n",
    );
}

#[then("publication reports an uncertain outcome and the same reviewed retry verifies accepted references")]
fn retry_uncertain_outcome(world: &World) {
    {
        let output = world.output.borrow();
        let output = output.as_ref().unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("outcome may be unknown"),
            "{output:?}"
        );
        assert!(output.stdout.is_empty());
    }
    assert_eq!(world.remote(&["rev-list", "--count", "isolock/v2"]), b"1\n");
    fs::remove_file(world.temp.path().join("remote.git/hooks/post-receive")).unwrap();
    exact_retry(world);
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Lost acknowledgement of an atomic publication is safely retryable"
)]
fn lost_acknowledgement_is_retryable(world: World) {
    let _ = world;
}

#[given("producer Git attributes would normalize the incoming original")]
fn producer_filters(world: &World) {
    fs::write(world.temp.path().join(".gitattributes"), "* text\n").unwrap();
    world.review("new", &b"binary\r\n".repeat(600));
    *world.before.borrow_mut() = world.capture();
}

#[then("the successor contains the exact reviewed CRLF original")]
fn exact_crlf_original(world: &World) {
    success(world.output.borrow().as_ref().unwrap());
    let original = b"binary\r\n".repeat(600);
    let path = format!(
        "isolock/v2:cas/{}",
        Sha256Digest::from_bytes(&original).sharded_path()
    );
    assert_eq!(world.remote(&["show", &path]), original);
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "Producer Git filters never transform reviewed originals"
)]
fn git_filters_do_not_change_reviewed_bytes(world: World) {
    let _ = world;
}

#[when("atomic support is restored and the same review is retried")]
fn restore_atomic_support(world: &World) {
    world.remote(&["config", "--unset", "receive.advertiseAtomic"]);
    publish(world);
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "An unaccepted atomic attempt can retry the same reviewed boundary"
)]
fn retry_after_unaccepted_atomic_attempt(world: World) {
    let _ = world;
}

#[scenario(
    path = "../../backlog/features/git-orphan-branch.feature",
    name = "A source race requires a new review rather than silently refreshing the pin"
)]
fn race_does_not_refresh_reviewed_pin(world: World) {
    let _ = world;
}
