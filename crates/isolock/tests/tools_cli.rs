//! The workflow `run:` steps delegate to `tools/cli.py`; guard what the tools task exports.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Fixture setup failures must fail the test.

use isolock_cas::Sha256Digest;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn tools_cli() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/cli.py")
}

/// Record every `cargo` invocation and the profile environment it inherited.
fn fake_cargo(directory: &Path) -> PathBuf {
    fs::create_dir_all(directory).unwrap();
    let log = directory.join("calls.txt");
    let script = format!(
        "#!/bin/sh\nset -eu\n{{ printf 'profile=%s\\n' \"${{ISOLOCK_TEST_PROFILE:-}}\"; \
printf 'platform=%s\\n' \"${{ISOLOCK_TEST_PLATFORM:-}}\"; \
printf 'args=%s\\n' \"$*\"; }} >> '{}'\n",
        log.display()
    );
    let cargo = directory.join("cargo");
    fs::write(&cargo, script).unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    log
}

#[test]
fn profile_materialize_exports_the_matrix_profile_to_the_test_runner() {
    let temp = tempfile::tempdir().unwrap();
    let tools = temp.path().join("fake-tools");
    let calls = fake_cargo(&tools);
    let output = Command::new("python3")
        .arg("-B")
        .arg(tools_cli())
        .args(["test", "profile-materialize"])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
        .env("HOME", temp.path())
        .env("ISOLOCK_TEST_PROFILE", "rust-standalone")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let log = fs::read_to_string(&calls).unwrap();
    assert!(log.contains("profile=rust-standalone"), "{log}");
    assert!(log.contains("platform=linux-64"), "{log}");
    assert!(
        log.contains("args=test --package isolock --test real_profile --offline"),
        "{log}"
    );
    assert!(
        log.contains("real_standalone_rust_builds_a_locked_registry_dependency_offline"),
        "{log}"
    );
}

#[test]
fn profile_journeys_reject_an_unknown_profile() {
    let output = Command::new("python3")
        .arg("-B")
        .arg(tools_cli())
        .args(["test", "profile-journeys", "--profile", "not-a-profile"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid choice"),
        "{output:?}"
    );
}

#[test]
fn release_check_tag_stays_silent_during_a_rehearsal() {
    let output = Command::new("python3")
        .arg("-B")
        .arg(tools_cli())
        .args(["release", "check-tag"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("RELEASE_PUBLISH", "false")
        .env("RELEASE_TAG", "")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("rehearsal"));
}

#[test]
fn docs_verify_installs_from_the_lockfile_before_building() {
    let temp = tempfile::tempdir().unwrap();
    let tools = temp.path().join("fake-tools");
    fs::create_dir_all(&tools).unwrap();
    let log = tools.join("calls.txt");
    let script = format!(
        "#!/bin/sh\nset -eu\nprintf 'args=%s\\n' \"$*\" >> '{}'\n",
        log.display()
    );
    let bun = tools.join("bun");
    fs::write(&bun, script).unwrap();
    fs::set_permissions(&bun, fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new("python3")
        .arg("-B")
        .arg(tools_cli())
        .args(["docs", "verify"])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
        .env("HOME", temp.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    // The gate must build from the reviewed lockfile, never a fresh resolution.
    let calls = fs::read_to_string(&log).unwrap();
    assert!(
        calls.contains("args=install --frozen-lockfile"),
        "the PR gate must install from the frozen lockfile: {calls}"
    );
    assert!(calls.contains("args=run build"), "{calls}");
}

#[test]
fn the_pull_request_pipeline_builds_the_documentation() {
    // The Pages workflow deploys from `main` only, so the PR pipeline is the
    // only place a broken MDX file can be caught before merge.
    let workflow = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/ci.yml"),
    )
    .unwrap();
    assert!(
        workflow.contains("pull_request:"),
        "the docs gate must run on pull requests"
    );
    assert!(
        workflow.contains("sh tools/run docs verify"),
        "the CI workflow must run the documentation build"
    );
}

/// Build a staged artifact directory the way `release stage-binaries` lays one out.
fn stage_artifact(directory: &Path, name: &str, contents: &[u8]) -> String {
    fs::create_dir_all(directory).unwrap();
    let file = directory.join(name);
    fs::write(&file, contents).unwrap();
    let digest = Sha256Digest::from_bytes(contents);
    fs::write(
        directory.join("SHA256SUMS"),
        format!("{}  {name}\n", digest.as_str()),
    )
    .unwrap();
    digest.as_str().to_owned()
}

#[test]
fn release_rejects_a_checksum_that_does_not_match_the_artifact() {
    // The whole point of publishing SHA256SUMS is that a mismatch stops the job. Prove the
    // gate fires rather than trusting that it is wired up.
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("binaries");
    stage_artifact(&directory, "isolock-linux-x86_64", b"real artifact bytes");
    fs::write(
        directory.join("SHA256SUMS"),
        format!("{}  isolock-linux-x86_64\n", "0".repeat(64)),
    )
    .unwrap();

    let output = Command::new("python3")
        .arg("-B")
        .arg(tools_cli())
        .args([
            "release",
            "verify-binary",
            "--directory",
            directory.to_str().unwrap(),
            "--archive-name",
            "absent.tar.gz",
            "--binary",
            "isolock",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("checksum mismatch for") && stderr.contains("isolock-linux-x86_64"),
        "the failure names the artifact: {stderr}"
    );
}

#[test]
fn release_rejects_a_checksum_manifest_naming_an_absent_artifact() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("binaries");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("SHA256SUMS"),
        format!("{}  isolock-linux-x86_64.tar.gz\n", "a".repeat(64)),
    )
    .unwrap();

    let output = Command::new("python3")
        .arg("-B")
        .arg(tools_cli())
        .args([
            "release",
            "verify-binary",
            "--directory",
            directory.to_str().unwrap(),
            "--archive-name",
            "isolock-linux-x86_64.tar.gz",
            "--binary",
            "isolock",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("missing file"),
        "{output:?}"
    );
}

#[test]
fn release_rejects_a_staging_directory_without_a_checksum_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("binaries");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("isolock-linux-x86_64.tar.gz"), b"bytes").unwrap();

    let output = Command::new("python3")
        .arg("-B")
        .arg(tools_cli())
        .args([
            "release",
            "verify-binary",
            "--directory",
            directory.to_str().unwrap(),
            "--archive-name",
            "isolock-linux-x86_64.tar.gz",
            "--binary",
            "isolock",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("missing checksum manifest"),
        "{output:?}"
    );
}

#[test]
fn conda_recipe_runs_a_documented_command_not_only_startup_checks() {
    // The install route has to prove more than that the package starts. `plan --ci` is the
    // drift verdict documented in the getting-started guide, so the recipe must run it.
    let recipe: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../recipe.yaml")).expect("recipe yaml parses");
    let script = recipe["tests"][0]["script"]
        .as_sequence()
        .expect("recipe declares a test script");
    let rendered = script
        .iter()
        .map(|entry| serde_yaml::to_string(entry).unwrap())
        .collect::<String>();
    for required in [
        "isolock --version",
        "isolock --help",
        "isolock config check",
    ] {
        assert!(rendered.contains(required), "recipe dropped {required}");
    }
    assert!(
        rendered.contains("isolock plan --ci"),
        "the recipe must run a representative documented command: {rendered}"
    );
    assert!(
        rendered.contains("Verdict: ALL DEPENDENCIES SATISFIED"),
        "the representative command must assert its verdict: {rendered}"
    );
}
