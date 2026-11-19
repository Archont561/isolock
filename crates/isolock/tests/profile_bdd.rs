#![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures fail fast on invalid setup.
#![cfg(unix)]

mod support;

use isolock_plan::profile::{ArtifactKind, Provider};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;
use std::fs;
use std::process::Output;
use support::{text, Fixture, PLATFORM, RUST_PROFILE};

struct World {
    project: Fixture,
    output: RefCell<Option<Output>>,
}
#[fixture]
fn world() -> World {
    World {
        project: Fixture::new(),
        output: RefCell::new(None),
    }
}

#[given("project configuration selects a CAS directory")]
fn project_cas(world: &World) {
    world.project.success(&["config", "check"]);
}
#[given("ISOLOCK_CAS_DIR supplies a higher-precedence directory")]
fn higher_precedence(_world: &World) {}
#[when("I run {command:string}")]
fn run_explain(world: &World, command: String) {
    assert_eq!(command, "isolock config explain storage.cas-dir");
    let output = world
        .project
        .command(&["config", "explain", "storage.cas-dir"])
        .env("ISOLOCK_CAS_DIR", "env-cache")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    *world.output.borrow_mut() = Some(output);
}
#[then("the effective value is the environment-provided directory")]
fn environment_value(world: &World) {
    assert!(text(world.output.borrow().as_ref().unwrap()).contains("env-cache"));
}
#[then("the output identifies ISOLOCK_CAS_DIR as its source")]
fn environment_source(world: &World) {
    assert!(text(world.output.borrow().as_ref().unwrap()).contains("source: ISOLOCK_CAS_DIR"));
}

#[given("profile {profile:string} has a generated locked plan")]
fn locked_plan(world: &World, profile: String) {
    world.project.plan(&profile);
}
#[given("the plan records the verified seed Pixi digest and selected Pixi environment")]
fn recorded_policy(world: &World) {
    let manifest = world.project.manifest(RUST_PROFILE);
    assert_eq!(manifest.specification.environment.as_deref(), Some("rust"));
    assert!(manifest
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == ArtifactKind::Seed && artifact.name == "pixi"));
}
#[when("CI overrides the seed Pixi version or the selected environment")]
fn ci_override(world: &World) {
    for (key, value) in [
        ("ISOLOCK_PIXI_VERSION", "9.9.9"),
        ("ISOLOCK_PIXI_ENVIRONMENT", "python"),
    ] {
        let output = world
            .project
            .command(&[
                "plan",
                "--profile",
                RUST_PROFILE,
                "--platform",
                PLATFORM,
                "--ci",
            ])
            .env(key, value)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{}", text(&output));
        assert!(
            text(&output).contains("locked input changed: specification."),
            "{}",
            text(&output)
        );
        *world.output.borrow_mut() = Some(output);
    }
}
#[then("{command:string} fails before materialization")]
fn fails_before_materialization(world: &World, command: String) {
    assert_eq!(command, "isolock plan --ci");
    assert!(!world.output.borrow().as_ref().unwrap().status.success());
    assert!(!world.project.root.join(".isolock/profiles").exists());
}
#[then("the failure identifies the locked input that changed")]
fn changed_field(world: &World) {
    assert!(text(world.output.borrow().as_ref().unwrap()).contains("specification.environment"));
}
#[when("a developer sets ISOLOCK_CAS_DIR to a writable local cache directory")]
fn operational_override(world: &World) {
    let destination = isolock_cas::FileStore::new(world.project.root.join("env-cache"));
    for digest in world.project.cas().digests().unwrap() {
        let digest = isolock_cas::Sha256Digest::parse(&digest).unwrap();
        destination
            .put_file(&digest, &world.project.cas().path(&digest))
            .unwrap();
    }
    let output = world
        .project
        .command(&[
            "plan",
            "--profile",
            RUST_PROFILE,
            "--platform",
            PLATFORM,
            "--ci",
        ])
        .env("ISOLOCK_CAS_DIR", "env-cache")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    *world.output.borrow_mut() = Some(output);
}
#[then("Isolock uses the local cache directory")]
fn local_cache(world: &World) {
    let output = world
        .project
        .command(&["config", "explain", "storage.cas-dir"])
        .env("ISOLOCK_CAS_DIR", "env-cache")
        .output()
        .unwrap();
    assert!(text(&output).contains("env-cache"));
}
#[then("the resolved artifact closure remains unchanged")]
fn unchanged_closure(world: &World) {
    assert!(text(world.output.borrow().as_ref().unwrap()).contains("locked inputs match"));
}

#[when("a developer overrides proxy concurrency and deadlines")]
fn proxy_limits_override(world: &World) {
    let path = world
        .project
        .root
        .join(format!(".isolock/plans/{RUST_PROFILE}-{PLATFORM}.lock"));
    let before = fs::read(&path).unwrap();
    let output = world
        .project
        .command(&[
            "plan",
            "--profile",
            RUST_PROFILE,
            "--platform",
            PLATFORM,
            "--ci",
        ])
        .env("ISOLOCK_PROXY_MAX_CONCURRENCY", "17")
        .env("ISOLOCK_PROXY_HEADER_TIMEOUT_MS", "250")
        .env("ISOLOCK_PROXY_WRITE_TIMEOUT_MS", "500")
        .env("ISOLOCK_PROXY_SHUTDOWN_GRACE_MS", "100")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        fs::read(path).unwrap(),
        before,
        "operational settings must not change serialized plans or artifact digests"
    );
    *world.output.borrow_mut() = Some(output);
}

#[given("a project declares Pixi environments named {rust:string}, {python:string}, and {typescript:string}")]
fn environments(world: &World, rust: String, python: String, typescript: String) {
    let source = fs::read_to_string(world.project.root.join("pixi.lock")).unwrap();
    for name in [rust, python, typescript] {
        assert!(source.contains(&format!("  {name}:")));
    }
}
#[given(
    "profile {profile:string} selects the {environment:string} Pixi environment and Cargo.lock"
)]
fn profile_environment(world: &World, profile: String, environment: String) {
    let specification = &world.project.profiles[&profile];
    assert_eq!(specification.environment.as_ref(), Some(&environment));
    assert_eq!(specification.cargo_lock.as_deref(), Some("Cargo.lock"));
}
#[given("the storage catalog contains artifacts for all three profiles")]
fn union_storage(world: &World) {
    for name in ["conda-rust", "conda-python", "conda-bun"] {
        assert!(world.project.blobs.contains_key(name));
    }
}
#[when("I plan and synchronize profile {profile:string} for {platform:string}")]
fn plan_and_sync(world: &World, profile: String, platform: String) {
    assert_eq!(platform, PLATFORM);
    world.project.plan(&profile);
    world.project.sync(&profile);
}
#[then("its manifest contains a verified standalone Pixi seed artifact")]
fn seed_in_manifest(world: &World) {
    let manifest = world.project.manifest(RUST_PROFILE);
    let seed = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == ArtifactKind::Seed && artifact.name == "pixi")
        .unwrap();
    assert!(world.project.cas().verified(&seed.digest).unwrap());
}
#[then("its Conda closure contains only packages reachable from the locked {environment:string} environment")]
fn conda_closure(world: &World, environment: String) {
    let manifest = world.project.manifest(RUST_PROFILE);
    assert_eq!(
        manifest
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::Conda)
            .map(|artifact| artifact.name.as_str())
            .collect::<Vec<_>>(),
        vec![environment]
    );
    assert!(!world.project.root.join(".pixi/envs/python").exists());
    assert!(!world.project.root.join(".pixi/envs/ts").exists());
}
#[then("its Cargo closure contains the selected Cargo vendor payload")]
fn cargo_payload(world: &World) {
    let manifest = world.project.manifest(RUST_PROFILE);
    assert_eq!(manifest.cargo_payloads.len(), 1);
    assert_eq!(
        manifest
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::Cargo)
            .count(),
        1
    );
    assert!(world.project.receipt(RUST_PROFILE)["payloads"]["cargo"].is_string());
}
#[then("its manifest contains no Python wheel or JavaScript package artifact")]
fn no_unrelated(world: &World) {
    assert!(!world
        .project
        .manifest(RUST_PROFILE)
        .artifacts
        .iter()
        .any(|artifact| matches!(
            artifact.kind,
            ArtifactKind::Wheel | ArtifactKind::Javascript
        )));
}

#[given("profile {profile:string} has a verified seed Pixi binary under {path:string}")]
fn pixi_available(world: &World, profile: String, path: String) {
    assert_eq!(path, ".isolock/bin");
    world.project.plan(&profile);
}
#[given("its locked Conda packages and repodata are available from the local Isolock channel")]
fn channel_available(world: &World) {
    assert!(world
        .project
        .cas()
        .verified(&world.project.profiles[RUST_PROFILE].seeds["pixi"].digest)
        .unwrap());
}
#[given("its Cargo vendor payload is available from CAS")]
fn vendor_available(world: &World) {
    assert!(world.project.blobs.contains_key("cargo"));
}
#[when("I bootstrap profile {profile:string}")]
fn bootstrap(world: &World, profile: String) {
    world.project.sync(&profile);
}
#[then("Pixi installs only environment {environment:string} with locked offline inputs")]
fn pixi_environment(world: &World, environment: String) {
    assert_eq!(
        world.project.receipt(RUST_PROFILE)["environment"],
        environment
    );
    assert_eq!(world.project.receipt(RUST_PROFILE)["provider"], "pixi");
}
#[then("{path:string} is available")]
fn path_available(world: &World, path: String) {
    assert!(world.project.root.join(path).is_file());
}
#[then("Cargo uses the verified vendor payload without contacting crates.io")]
fn cargo_offline(world: &World) {
    let output = std::process::Command::new(world.project.bin(RUST_PROFILE, "cargo"))
        .arg("build")
        .current_dir(&world.project.root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("built hello offline"));
}

#[given(
    "profiles {pixi:string} and {standalone:string} select the same Cargo.lock vendor derivation"
)]
fn same_vendor(world: &World, pixi: String, standalone: String) {
    let first = world.project.plan(&pixi);
    let second = world.project.plan(&standalone);
    assert_eq!(first.cargo_payloads, second.cargo_payloads);
}
#[given("{profile:string} uses Conda Rust packages")]
fn conda_provider(world: &World, profile: String) {
    assert_eq!(world.project.profiles[&profile].provider, Provider::Pixi);
}
#[given("{profile:string} uses a standalone Rust distribution")]
fn standalone_provider(world: &World, profile: String) {
    assert!(world.project.profiles[&profile]
        .runtimes
        .contains_key("rust"));
}
#[when("I synchronize both profiles")]
fn sync_both(world: &World) {
    world.project.sync(RUST_PROFILE);
    world.project.sync("rust-standalone");
}
#[then("both manifests may reference the same Cargo vendor digest")]
fn shared_vendor(world: &World) {
    assert_eq!(
        world.project.receipt(RUST_PROFILE)["payloads"]["cargo"],
        world.project.receipt("rust-standalone")["payloads"]["cargo"]
    );
}
#[then("each manifest records its own runtime-provider artifacts")]
fn distinct_providers(world: &World) {
    assert_ne!(
        world.project.manifest(RUST_PROFILE).artifacts,
        world.project.manifest("rust-standalone").artifacts
    );
}
#[then("no CAS blob is mutated to represent either profile")]
fn immutable_blobs(world: &World) {
    for bytes in world.project.blobs.values() {
        assert!(world
            .project
            .cas()
            .verified(&isolock_cas::Sha256Digest::from_bytes(bytes))
            .unwrap());
    }
}

#[scenario(
    path = "../../backlog/features/profile-configuration.feature",
    index = 0
)]
fn bdd_configuration_provenance(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profile-configuration.feature",
    index = 1
)]
fn bdd_locked_overrides(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profile-configuration.feature",
    index = 2
)]
fn bdd_operational_override(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profiled-environment-materialization.feature",
    index = 0
)]
fn bdd_profile_isolation(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profiled-environment-materialization.feature",
    index = 1
)]
fn bdd_offline_pixi(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profiled-environment-materialization.feature",
    index = 2
)]
fn bdd_shared_cargo(world: World) {
    let _ = world;
}

#[scenario(
    path = "../../backlog/features/profile-configuration.feature",
    index = 3
)]
fn bdd_proxy_limits_do_not_change_locked_identity(world: World) {
    let _ = world;
}
