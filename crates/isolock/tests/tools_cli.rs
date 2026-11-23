//! The workflow `run:` steps delegate to `tools/cli.py`; guard what the tools task exports.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Fixture setup failures must fail the test.

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
