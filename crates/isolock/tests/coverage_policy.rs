//! Keep Codecov's publishing scope aligned with the instrumented Cargo workspace.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid policy fixtures must fail the test.

use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use serde_yaml::Value;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

const SCRIPT: &str = include_str!("../../../scripts/coverage.sh");

struct CoverageWorld {
    members: Vec<PathBuf>,
    codecov: Value,
    workflow: Value,
    scope: RefCell<Vec<PathBuf>>,
}

#[fixture]
fn world() -> CoverageWorld {
    let manifest: toml::Value = toml::from_str(include_str!("../../../Cargo.toml")).unwrap();
    let members = manifest["workspace"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| PathBuf::from(member.as_str().unwrap()))
        .collect();
    CoverageWorld {
        members,
        codecov: serde_yaml::from_str(include_str!("../../../.codecov.yml")).unwrap(),
        workflow: serde_yaml::from_str(include_str!("../../../.github/workflows/ci.yml")).unwrap(),
        scope: RefCell::new(Vec::new()),
    }
}

fn configured_scope(policy: &Value) -> Vec<PathBuf> {
    policy["flags"]["rust"]["paths"]
        .as_sequence()
        .expect("the rust flag must have an explicit workspace directory scope")
        .iter()
        .map(|prefix| PathBuf::from(prefix.as_str().unwrap()))
        .collect()
}

fn missing_sources(members: &[PathBuf], scope: &[PathBuf]) -> Vec<PathBuf> {
    members
        .iter()
        .map(|member| member.join("src"))
        .filter(|source| !scope.iter().any(|prefix| source.starts_with(prefix)))
        .collect()
}

#[given("the declared Rust workspace crates")]
fn declared_workspace(world: &CoverageWorld) {
    assert!(!world.members.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for member in &world.members {
        assert!(root.join(member).join("src").is_dir(), "{member:?}");
    }
}

#[when("the CI coverage publishing policy is inspected")]
fn inspect_policy(world: &CoverageWorld) {
    *world.scope.borrow_mut() = configured_scope(&world.codecov);
}

#[then("the rust coverage flag includes every workspace crate source directory")]
fn all_crates_in_scope(world: &CoverageWorld) {
    let missing = missing_sources(&world.members, &world.scope.borrow());
    assert!(
        missing.is_empty(),
        "workspace sources excluded from the rust flag: {missing:?}"
    );
}

#[then("every configured coverage scope directory exists")]
fn existing_scope(world: &CoverageWorld) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for prefix in world.scope.borrow().iter() {
        assert!(
            prefix.is_relative(),
            "coverage roots must be repository-relative"
        );
        assert!(
            root.join(prefix).is_dir(),
            "stale coverage root: {prefix:?}"
        );
    }
}

#[then("CI generates and uploads the same full-workspace LCOV report")]
fn complete_report(world: &CoverageWorld) {
    let steps = world.workflow["jobs"]["test-and-coverage"]["steps"]
        .as_sequence()
        .unwrap();
    assert!(steps.iter().any(|step| step["run"]
        .as_str()
        .is_some_and(|command| command.ends_with("sh scripts/coverage.sh"))));
    let command = SCRIPT
        .lines()
        .find(|line| line.starts_with("exec cargo llvm-cov "))
        .unwrap();
    let arguments = command.split_whitespace().collect::<Vec<_>>();
    for required in ["--workspace", "--all-features", "--offline", "--lcov"] {
        assert!(
            arguments.contains(&required),
            "coverage must include {required}"
        );
    }
    assert!(!arguments
        .iter()
        .any(|argument| matches!(*argument, "--package" | "-p" | "--exclude")));
    let report = arguments
        .windows(2)
        .find_map(|pair| (pair[0] == "--output-path").then_some(pair[1]))
        .unwrap();
    let uploader = steps
        .iter()
        .find(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|action| action.starts_with("codecov/codecov-action@"))
        })
        .unwrap();
    assert_eq!(uploader["with"]["files"].as_str(), Some(report));
    assert_eq!(uploader["with"]["flags"].as_str(), Some("rust"));
    assert_eq!(uploader["with"]["disable_search"].as_bool(), Some(true));
}

#[then("previous reports cannot replace missing current coverage")]
fn fresh_report(world: &CoverageWorld) {
    assert_eq!(
        world.codecov["flags"]["rust"]["carryforward"].as_bool(),
        Some(false)
    );
}

#[scenario(path = "../../backlog/features/workspace-coverage.feature", index = 0)]
fn bdd_full_workspace_coverage_policy(world: CoverageWorld) {
    let _ = world;
}

#[test]
fn regression_detects_the_old_cli_only_scope_and_stale_version_crate() {
    let world = world();
    let legacy = [
        PathBuf::from("crates/isolock/src/"),
        PathBuf::from("crates/isolock-version/src/"),
    ];
    let missing = missing_sources(&world.members, &legacy);
    for member in world
        .members
        .iter()
        .filter(|member| member.as_path() != Path::new("crates/isolock"))
    {
        assert!(
            missing.contains(&member.join("src")),
            "failed to detect excluded crate: {member:?}"
        );
    }
    assert!(!missing.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(!root.join(&legacy[1]).is_dir());
}

#[test]
fn future_crates_under_the_workspace_root_are_included_automatically() {
    let world = world();
    let future = vec![PathBuf::from("crates/future-isolock-crate")];
    assert!(missing_sources(&future, &configured_scope(&world.codecov)).is_empty());
}
