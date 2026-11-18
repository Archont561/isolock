#![cfg(unix)]

mod support;

use isolock_cas::{FileStore, Sha256Digest};
use isolock_plan::profile::ArchiveFormat;
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use support::{archive, text, Fixture, PLATFORM, RUST_PROFILE};

struct World {
    project: RefCell<Fixture>,
    other: RefCell<Option<Fixture>>,
    output: RefCell<Option<Output>>,
    report: RefCell<Option<serde_json::Value>>,
}
#[fixture]
fn world() -> World {
    World {
        project: RefCell::new(Fixture::new()),
        other: RefCell::new(None),
        output: RefCell::new(None),
        report: RefCell::new(None),
    }
}

#[given("verified standalone profile {profile:string}")]
fn standalone(world: &World, profile: String) {
    world.project.borrow().plan(&profile);
}
#[when("I materialize the standalone profile {profile:string}")]
fn materialize(world: &World, profile: String) {
    world.project.borrow().sync(&profile);
}
#[then("its {program:string} launcher builds with the locked offline vendor source")]
fn vendor_build(world: &World, program: String) {
    let project = world.project.borrow();
    let output = Command::new(project.bin("rust-standalone", &program))
        .arg("build")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("built hello offline"));
    let receipt = project.receipt("rust-standalone");
    let vendor = PathBuf::from(receipt["payloads"]["cargo"].as_str().unwrap());
    let checksums: serde_json::Value =
        serde_json::from_slice(&fs::read(vendor.join("hello-1.0.0/.cargo-checksum.json")).unwrap())
            .unwrap();
    assert!(checksums["files"]["src/lib.rs"].is_string());
    assert!(checksums["package"].is_string());
}
#[then("its Isolock launcher refers to a version-owned verified seed")]
fn seed_launcher(world: &World) {
    let project = world.project.borrow();
    let script = fs::read_to_string(project.bin("rust-standalone", "isolock")).unwrap();
    assert!(script.contains("/.isolock/bin/.seeds/"));
    assert!(script.contains("exec "));
    assert_eq!(project.receipt("rust-standalone")["provider"], "standalone");
}
#[then("Python and pip expose only the locked wheelhouse installation flow")]
fn python_wheels(world: &World) {
    let project = world.project.borrow();
    for (program, args) in [("python", vec!["--version"]), ("pip", vec!["install"])] {
        let output = Command::new(project.bin("python-standalone", program))
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", text(&output));
        if program == "pip" {
            for option in [
                "--no-index",
                "--no-deps",
                "--require-hashes",
                "--find-links",
            ] {
                assert!(text(&output).contains(option));
            }
        }
    }
    let output = Command::new(project.bin("python-standalone", "pip"))
        .args(["install", "https://evil.invalid/not-locked.whl"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let receipt = project.receipt("python-standalone");
    let wheels = PathBuf::from(receipt["payloads"]["pypi"].as_str().unwrap());
    assert!(wheels.join("demo-1.0.0-py3-none-any.whl").is_file());
    project.sync("python-standalone");
}
#[then("bun x executes the restored executable {executable:string}")]
fn bun_locked(world: &World, executable: String) {
    let output = Command::new(world.project.borrow().bin("bun-standalone", "bun"))
        .args(["x", &executable])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("locked-hello"));
}
#[then("bun x rejects the unrecorded executable {executable:string} without calling the runtime")]
fn bun_missing(world: &World, executable: String) {
    let output = Command::new(world.project.borrow().bin("bun-standalone", "bun"))
        .args(["x", &executable])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(text(&output).contains("not in the locked profile"));
    assert!(!text(&output).contains("bun-runtime"));
}

#[given("a directory snapshot containing Rust and unrelated profile blobs")]
fn union_snapshot(world: &World) {
    let project = world.project.borrow();
    project.plan(RUST_PROFILE);
    let source = project.temp.path().join("source");
    project.success(&[
        "export",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--output",
        source.to_str().unwrap(),
    ]);
    let store = FileStore::new(source.join("cas"));
    for bytes in project.blobs.values() {
        store
            .put_bytes(&Sha256Digest::from_bytes(bytes), bytes)
            .unwrap();
    }
    project.write_config(Some(&source));
    fs::remove_dir_all(project.cas().root()).unwrap();
}
#[when("I fetch the selected Rust manifest into an empty CAS")]
fn fetch_rust(world: &World) {
    let project = world.project.borrow();
    let digest = project.manifest(RUST_PROFILE).digest().unwrap();
    let output = project.success(&[
        "fetch",
        "--profile",
        RUST_PROFILE,
        "--platform",
        PLATFORM,
        "--manifest",
        digest.as_str(),
    ]);
    *world.output.borrow_mut() = Some(output);
}
#[then("only blobs reachable from the Rust manifest are copied")]
fn selective(world: &World) {
    let project = world.project.borrow();
    assert_eq!(
        project.cas().digests().unwrap(),
        project.manifest(RUST_PROFILE).reachable_blobs()
    );
    assert!(!project
        .cas()
        .verified(&Sha256Digest::from_bytes(&project.blobs["conda-python"]))
        .unwrap());
}
#[then("a missing source manifest prevents any payload copy")]
fn manifest_first(world: &World) {
    let project = world.project.borrow();
    let digest = project.manifest(RUST_PROFILE).digest().unwrap();
    fs::remove_file(
        project
            .temp
            .path()
            .join(format!("source/plans/objects/{}.json", digest.as_str())),
    )
    .unwrap();
    fs::remove_dir_all(project.cas().root()).unwrap();
    let output = project.run(&["fetch", "--profile", RUST_PROFILE, "--platform", PLATFORM]);
    assert!(!output.status.success());
    assert!(text(&output).contains("selected manifest"));
    assert!(project.cas().digests().unwrap().is_empty());
}

#[given("two workspaces register different profile roots in a shared CAS")]
fn shared_cas(world: &World) {
    let project = world.project.borrow();
    let other = Fixture::new();
    let shared = project.temp.path().join("shared-cas");
    let store = FileStore::new(&shared);
    for project in [&*project, &other] {
        for bytes in project.blobs.values() {
            store
                .put_bytes(&Sha256Digest::from_bytes(bytes), bytes)
                .unwrap();
        }
    }
    for (project, profile) in [(&*project, RUST_PROFILE), (&other, "python-standalone")] {
        let output = project
            .command(&["plan", "--profile", profile, "--platform", PLATFORM])
            .env("ISOLOCK_CAS_DIR", &shared)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", text(&output));
    }
    store
        .put_bytes(&Sha256Digest::from_bytes(b"garbage"), b"garbage")
        .unwrap();
    *world.other.borrow_mut() = Some(other);
}
#[when("I collect garbage for one workspace")]
fn collect(world: &World) {
    let project = world.project.borrow();
    let shared = project.temp.path().join("shared-cas");
    let output = project
        .command(&["gc", "--apply"])
        .env("ISOLOCK_CAS_DIR", &shared)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    *world.report.borrow_mut() = Some(serde_json::from_slice(&output.stdout).unwrap());
}
#[then("both workspaces retain their reachable artifacts")]
fn retains_both(world: &World) {
    let project = world.project.borrow();
    let other = world.other.borrow();
    let other = other.as_ref().unwrap();
    let store = FileStore::new(project.temp.path().join("shared-cas"));
    for digest in project
        .manifest(RUST_PROFILE)
        .reachable_blobs()
        .union(&other.manifest("python-standalone").reachable_blobs())
    {
        assert!(store
            .verified(&Sha256Digest::parse(digest).unwrap())
            .unwrap());
    }
}
#[then("the reclaimable blob report is deterministic")]
fn deterministic_gc(world: &World) {
    let report = world.report.borrow();
    let report = report.as_ref().unwrap();
    let reclaimable = report["reclaimable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|digest| digest.as_str().unwrap())
        .collect::<Vec<_>>();
    let sorted = reclaimable
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    assert_eq!(reclaimable, sorted);
    assert!(reclaimable.contains(&Sha256Digest::from_bytes(b"garbage").as_str()));
    let project = world.project.borrow();
    let output = project
        .command(&["gc"])
        .env("ISOLOCK_CAS_DIR", project.temp.path().join("shared-cas"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let second: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(second["reclaimable"], serde_json::json!([]));
}

#[given("the locked Pixi seed is corrupt and a host Pixi is available")]
fn corrupt_seed(world: &World) {
    let project = world.project.borrow();
    project.plan(RUST_PROFILE);
    let seed = &project.profiles[RUST_PROFILE].seeds["pixi"].digest;
    fs::write(project.cas().path(seed), b"corrupt seed").unwrap();
    fs::create_dir_all(project.root.join("host-bin")).unwrap();
    fs::write(
        project.root.join("host-bin/pixi"),
        "#!/bin/sh\ntouch host-pixi-executed\n",
    )
    .unwrap();
    fs::set_permissions(
        project.root.join("host-bin/pixi"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
}
#[when("I attempt locked profile synchronization")]
fn sync_corrupt(world: &World) {
    let project = world.project.borrow();
    let output = project
        .command(&[
            "sync",
            "--profile",
            RUST_PROFILE,
            "--platform",
            PLATFORM,
            "--locked",
            "--offline",
        ])
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", project.root.join("host-bin").display()),
        )
        .output()
        .unwrap();
    *world.output.borrow_mut() = Some(output);
}
#[then("synchronization fails before executing Pixi or creating a prefix")]
fn no_host_fallback(world: &World) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert!(!output.status.success());
    assert!(text(output).contains("mismatch"));
    let project = world.project.borrow();
    assert!(!project.root.join("host-pixi-executed").exists());
    assert!(!project.root.join(".pixi/envs").exists());
}

#[given("a reviewed bootstrap manifest with an archived Isolock seed")]
fn archived_seed(world: &World) {
    let mut project = world.project.borrow_mut();
    let bytes = archive(
        &[(
            "tools/isolock",
            "#!/bin/sh\nprintf '%s\\n' \"$0\" \"$*\" > seed-executed\n",
            true,
        )],
        "-czf",
    );
    let mut tool = project.tool(
        "archived-seed",
        "isolock.tar.gz",
        &bytes,
        ArchiveFormat::TarGz,
    );
    tool.entry = Some("tools/isolock".into());
    project
        .profiles
        .get_mut("rust-standalone")
        .unwrap()
        .seeds
        .insert("isolock".into(), tool);
    project.write_config(None);
    project.plan("rust-standalone");
    project.success(&[
        "seed-manifest",
        "--profile",
        "rust-standalone",
        "--platform",
        PLATFORM,
    ]);
}
#[when("I run the POSIX profile bootstrap without a language runtime")]
fn bootstrap_shell(world: &World) {
    let project = world.project.borrow();
    let output = Command::new("/bin/sh")
        .arg(project.root.join(".isolock/bootstrap.sh"))
        .args([
            "--profile",
            "rust-standalone",
            "--platform",
            PLATFORM,
            "--offline",
        ])
        .env("PATH", "/usr/bin:/bin")
        .env(
            "TAR_OPTIONS",
            "--checkpoint=1 --checkpoint-action=exec=touch should-not-execute",
        )
        .current_dir(&project.root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    *world.output.borrow_mut() = Some(output);
}
#[then("only the verified version-owned seed is executed")]
fn only_seed(world: &World) {
    let project = world.project.borrow();
    let log = fs::read_to_string(project.root.join("seed-executed")).unwrap();
    assert!(log.contains("/.isolock/bin/.seeds/"));
    assert!(log.contains("sync --profile rust-standalone --platform linux-64 --locked --offline"));
    assert!(!project.root.join("should-not-execute").exists());
}

#[given("project configuration contains an inline registry credential")]
fn inline_credential(world: &World) {
    let mut project = world.project.borrow_mut();
    project
        .profiles
        .get_mut(RUST_PROFILE)
        .unwrap()
        .seeds
        .get_mut("pixi")
        .unwrap()
        .source = "https://user:super-secret-password@github.com/fixture".into();
    project.write_config(None);
}
#[when("I validate render and explain the configuration")]
fn credential_commands(world: &World) {
    let project = world.project.borrow();
    for args in [
        vec!["config", "check"],
        vec!["config", "show"],
        vec!["config", "explain", "profiles.rust-pixi.seeds.pixi.source"],
        vec!["plan", "--profile", RUST_PROFILE, "--platform", PLATFORM],
    ] {
        let output = project.run(&args);
        assert!(!output.status.success());
        assert!(!text(&output).contains("super-secret-password"));
        *world.output.borrow_mut() = Some(output);
    }
}
#[then("every diagnostic omits the credential")]
fn credential_absent(world: &World) {
    assert!(!text(world.output.borrow().as_ref().unwrap()).contains("super-secret-password"));
    assert!(!world.project.borrow().root.join(".isolock/plans").exists());
}

#[scenario(
    path = "../../backlog/features/profile-transport-and-safety.feature",
    index = 0
)]
fn bdd_standalone_rust(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profile-transport-and-safety.feature",
    index = 1
)]
fn bdd_standalone_python(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profile-transport-and-safety.feature",
    index = 2
)]
fn bdd_standalone_javascript(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profile-transport-and-safety.feature",
    index = 3
)]
fn bdd_selective_directory_transport(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profile-transport-and-safety.feature",
    index = 4
)]
fn bdd_shared_cas_gc(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profile-transport-and-safety.feature",
    index = 5
)]
fn bdd_seed_trust_boundary(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profile-transport-and-safety.feature",
    index = 6
)]
fn bdd_posix_seed_bootstrap(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/profile-transport-and-safety.feature",
    index = 7
)]
fn bdd_credentials_never_render(world: World) {
    let _ = world;
}

#[test]
fn configuration_layers_are_ordered_and_unknown_fields_fail_closed() {
    let project = Fixture::new();
    let user = project.root.join("user.toml");
    fs::write(&user, "[storage]\ncas-dir='user-cache'\n").unwrap();
    fs::create_dir_all(project.root.join(".isolock")).unwrap();
    fs::write(
        project.root.join(".isolock/config.local.toml"),
        "[storage]\ncas-dir='local-cache'\n",
    )
    .unwrap();
    let output = project
        .command(&[
            "config",
            "explain",
            "storage.cas-dir",
            "--user-config",
            user.to_str().unwrap(),
        ])
        .env("ISOLOCK_CAS_DIR", "env-cache")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("env-cache"));
    let output = project
        .command(&[
            "config",
            "explain",
            "storage.cas-dir",
            "--cas-dir",
            "flag-cache",
            "--user-config",
            user.to_str().unwrap(),
        ])
        .env("ISOLOCK_CAS_DIR", "env-cache")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("flag-cache"));
    assert!(text(&output).contains("explicit CLI flag"));
    let output = project.success(&[
        "config",
        "explain",
        "storage.cas-dir",
        "--user-config",
        user.to_str().unwrap(),
    ]);
    assert!(text(&output).contains("generated project-local configuration"));
    fs::write(&user, "[profiles.injected]\nprovider='standalone'\n").unwrap();
    let output = project.run(&["config", "show", "--user-config", user.to_str().unwrap()]);
    assert!(!output.status.success());
    fs::write(
        project.root.join(".isolock/config.local.toml"),
        "unknown-secret-field='DO-NOT-PRINT-THIS'\n",
    )
    .unwrap();
    let output = project.run(&["config", "show"]);
    assert!(!output.status.success());
    assert!(!text(&output).contains("DO-NOT-PRINT-THIS"));
}

#[test]
fn composed_plans_are_deterministic_and_byte_deduplicated() {
    let project = Fixture::new();
    let first = project.plan("all");
    let second = project.plan("all");
    assert_eq!(first, second);
    assert_eq!(first.constituents.len(), 3);
    let source = project.temp.path().join("export");
    project.success(&[
        "export",
        "--profile",
        "all",
        "--platform",
        PLATFORM,
        "--output",
        source.to_str().unwrap(),
    ]);
    assert_eq!(
        FileStore::new(source.join("cas")).digests().unwrap(),
        first.reachable_blobs()
    );
    project.sync("all");
    assert!(project.bin("all", "cargo").is_file());
    assert!(project.bin("all", "python").is_file());
    assert!(project.bin("all", "bun").is_file());
}

#[test]
fn cargo_derivations_change_for_every_policy_input_and_runtime_provenance_is_separate() {
    let mut project = Fixture::new();
    let original = project.plan("rust-standalone");
    let original_key = original.cargo_payloads.keys().next().unwrap().clone();
    for field in ["features", "target", "cargo-version", "all-features"] {
        let mut changed = project.profiles["rust-standalone"].clone();
        match field {
            "features" => changed.cargo.features.push("feature".into()),
            "target" => changed.cargo.target = "aarch64-unknown-linux-gnu".into(),
            "cargo-version" => changed.cargo.cargo_version = "9.0.0".into(),
            _ => changed.cargo.all_features = true,
        };
        let previous = project
            .profiles
            .insert("rust-standalone".into(), changed)
            .unwrap();
        project.write_config(None);
        let manifest = project.plan("rust-standalone");
        assert_ne!(
            manifest.cargo_payloads.keys().next().unwrap(),
            &original_key
        );
        project.profiles.insert("rust-standalone".into(), previous);
    }
}

#[test]
fn modified_inputs_catalogs_and_cached_trees_are_rejected() {
    let project = Fixture::new();
    project.plan("bun-standalone");
    project.sync("bun-standalone");
    let receipt = project.receipt("bun-standalone");
    let payload = PathBuf::from(receipt["payloads"]["javascript"].as_str().unwrap());
    fs::write(payload.join("node_modules/hello-cli/cli.sh"), "changed").unwrap();
    let output = project.run(&[
        "sync",
        "--profile",
        "bun-standalone",
        "--platform",
        PLATFORM,
        "--offline",
    ]);
    assert!(!output.status.success());
    assert!(text(&output).contains("modified"));
    fs::write(project.root.join("catalog.json"), "[]").unwrap();
    let output = project.run(&[
        "plan",
        "--profile",
        "bun-standalone",
        "--platform",
        PLATFORM,
        "--ci",
    ]);
    assert!(!output.status.success());
}

#[test]
fn cycles_future_schemas_and_missing_runtime_capabilities_are_rejected() {
    let mut project = Fixture::new();
    project.profiles.get_mut("all").unwrap().compose = vec!["all".into()];
    project.write_config(None);
    let output = project.run(&["plan", "--profile", "all", "--platform", PLATFORM]);
    assert!(!output.status.success());
    assert!(text(&output).contains("cycle"));
    let source = fs::read_to_string(project.root.join("isolock.toml"))
        .unwrap()
        .replace("schema-version = 1", "schema-version = 999");
    fs::write(project.root.join("isolock.toml"), source).unwrap();
    let output = project.run(&["config", "check"]);
    assert!(!output.status.success());
    assert!(text(&output).contains("schema-version"));
    project.profiles.get_mut("all").unwrap().compose = vec![RUST_PROFILE.into()];
    let bytes = archive(&[("bin/rustc", "#!/bin/sh\necho rustc\n", true)], "-czf");
    let runtime = project.tool(
        "incomplete-rust",
        "rust.tar.gz",
        &bytes,
        ArchiveFormat::TarGz,
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
    assert!(!output.status.success());
    assert!(text(&output).contains("bin/cargo"));
}
