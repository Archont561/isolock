//! Real CLI lifecycle regressions for the execution-session contract.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures fail fast on invalid setup.

mod coverage_support;

use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::Duration;
use tempfile::TempDir;

struct Project {
    temp: TempDir,
    address: SocketAddr,
}
impl Project {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("temporary")).unwrap();
        fs::write(temp.path().join("pixi.toml"), "tls-no-verify = false\n").unwrap();
        // Reserve an OS-chosen port briefly; each process then binds that resolved port.
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        Self { temp, address }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .env_clear()
            .current_dir(self.temp.path())
            .args(args)
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.temp.path())
            .env("TMPDIR", self.temp.path().join("temporary"))
            .env("PIXI_CONFIG_FILE", self.temp.path().join("pixi.toml"))
            .env("PIXI_NO_CONFIG", "1")
            .env("NO_PROXY", "inherited.invalid")
            .env("ISOLOCK_PROXY_PORT", self.address.port().to_string())
            .env("ISOLOCK_CARGO_HOME", self.temp.path().join("cargo home"));
        coverage_support::preserve_profile(&mut command);
        command
    }
    fn assert_cleaned(&self) {
        assert!(TcpStream::connect_timeout(&self.address, Duration::from_millis(100)).is_err());
        assert_eq!(
            fs::read_dir(self.temp.path().join("temporary"))
                .unwrap()
                .count(),
            0
        );
        assert!(!self.temp.path().join("child-started").exists());
    }
}

#[derive(Default)]
struct World {
    project: RefCell<Option<Project>>,
    output: RefCell<Option<Output>>,
}
#[fixture]
fn world() -> World {
    World::default()
}

#[given("an isolated execution project with inherited Pixi configuration")]
fn project(world: &World) {
    *world.project.borrow_mut() = Some(Project::new());
}

#[when("I run a child with ergonomic isolock exec")]
fn child_runs(world: &World) {
    let project = world.project.borrow();
    let project = project.as_ref().unwrap();
    let script = r#"
set -eu
[ "$HTTP_PROXY" = "$ISOLOCK_PROXY_URL" ]
[ "$https_proxy" = "$ISOLOCK_PROXY_URL" ]
[ "$ALL_PROXY" = "$ISOLOCK_PROXY_URL" ]
[ "$NPM_CONFIG_REGISTRY" = "$ISOLOCK_PROXY_URL/npm/" ]
[ "$PIP_INDEX_URL" = "$ISOLOCK_PROXY_URL/pypi/simple" ]
[ "$UV_DEFAULT_INDEX" = "$PIP_INDEX_URL" ]
[ -z "$PIP_EXTRA_INDEX_URL$UV_EXTRA_INDEX_URL" ]
[ "$CARGO_REGISTRIES_CRATES_IO_INDEX" = "sparse+$ISOLOCK_PROXY_URL/cargo/" ]
[ "$PIXI_OFFLINE" = false ]
[ "${PIXI_NO_CONFIG+x}" != x ]
[ "$NO_PROXY" = '127.0.0.1,localhost,::1,inherited.invalid' ]
[ "$NO_PROXY" = "$no_proxy" ]
[ -f "$CARGO_HOME/config.toml" ]
[ -f "$PIXI_CONFIG_FILE" ]
printf '%s\n' "$ISOLOCK_PROXY_URL" "$PIXI_CONFIG_FILE"
exit 7
"#;
    *world.output.borrow_mut() = Some(
        project
            .command(&["exec", "/bin/sh", "-c", script])
            .output()
            .unwrap(),
    );
}

#[then("the child receives the composed local registry environment")]
fn child_environment(world: &World) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "{output:?}");
    assert_eq!(
        lines[0],
        format!(
            "http://{}",
            world.project.borrow().as_ref().unwrap().address
        )
    );
    assert!(
        !PathBuf::from(lines[1]).exists(),
        "the child's generated Pixi file outlived the session"
    );
}
#[then("its exit code 7 is propagated unchanged")]
fn child_exit(world: &World) {
    assert_eq!(
        world.output.borrow().as_ref().unwrap().status.code(),
        Some(7)
    );
}

#[when("the requested child executable does not exist")]
fn missing_child(world: &World) {
    *world.output.borrow_mut() = Some(
        world
            .project
            .borrow()
            .as_ref()
            .unwrap()
            .command(&["exec", "--", "/nonexistent-isolock-m6-child"])
            .output()
            .unwrap(),
    );
}
#[then("execution reports a child startup failure")]
fn startup_error(world: &World) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot start"));
}

#[when("execution encounters invalid inherited Pixi mirrors")]
fn pixi_failure(world: &World) {
    let project = world.project.borrow();
    let project = project.as_ref().unwrap();
    fs::write(project.temp.path().join("pixi.toml"), "mirrors = 42\n").unwrap();
    *world.output.borrow_mut() = Some(
        project
            .command(&["exec", "/bin/sh", "-c", "touch child-started"])
            .output()
            .unwrap(),
    );
    assert!(
        project.temp.path().join("cargo home/config.toml").exists(),
        "setup must progress past proxy startup and Cargo configuration"
    );
}
#[when("execution cannot create the resolved Cargo home")]
fn cargo_failure(world: &World) {
    let project = world.project.borrow();
    let project = project.as_ref().unwrap();
    fs::write(project.temp.path().join("cargo home"), "not a directory").unwrap();
    *world.output.borrow_mut() = Some(
        project
            .command(&["exec", "/bin/sh", "-c", "touch child-started"])
            .output()
            .unwrap(),
    );
}
#[then("execution fails before starting the child")]
fn failed_before_child(world: &World) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot configure"));
    assert!(!world
        .project
        .borrow()
        .as_ref()
        .unwrap()
        .temp
        .path()
        .join("child-started")
        .exists());
}
#[then("the local proxy and temporary Pixi configuration are cleaned up")]
fn cleanup(world: &World) {
    world.project.borrow().as_ref().unwrap().assert_cleaned();
}

#[scenario(path = "../../backlog/features/execution-sessions.feature", index = 0)]
fn bdd_child_environment_and_exit(world: World) {
    let _ = world;
}
#[scenario(path = "../../backlog/features/execution-sessions.feature", index = 1)]
fn bdd_child_start_failure_cleanup(world: World) {
    let _ = world;
}
#[scenario(path = "../../backlog/features/execution-sessions.feature", index = 2)]
fn bdd_pixi_failure_cleanup(world: World) {
    let _ = world;
}
#[scenario(path = "../../backlog/features/execution-sessions.feature", index = 3)]
fn bdd_cargo_failure_cleanup(world: World) {
    let _ = world;
}

#[test]
fn separator_child_version_flags_and_signal_status_are_not_consumed_by_isolock() {
    let project = Project::new();
    for prefix in [vec!["exec"], vec!["exec", "--"]] {
        let mut args = prefix;
        args.extend([
            "/bin/sh",
            "-c",
            "printf '%s' \"$1\"; exit 9",
            "fixture",
            "--version",
        ]);
        let output = project.command(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(9), "{output:?}");
        assert_eq!(output.stdout, b"--version");
        project.assert_cleaned();
    }
    let output = project
        .command(&["exec", "/bin/sh", "-c", "kill -TERM $$"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(143));
    project.assert_cleaned();
}

#[test]
fn execution_uses_layered_proxy_inputs_and_explicit_missing_plans_fail_closed() {
    let project = Project::new();
    fs::write(
        project.temp.path().join("isolock.toml"),
        "[proxy]\nplan='absent-plan.json'\n",
    )
    .unwrap();
    let output = project
        .command(&["exec", "/bin/sh", "-c", "touch child-started"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read plan"));
    project.assert_cleaned();
}

#[test]
fn setup_failure_after_socket_bind_does_not_leave_a_listener() {
    let project = Project::new();
    // A malformed inherited config is rejected after the proxy is live, before spawning a child.
    let mut original = fs::OpenOptions::new()
        .append(true)
        .open(project.temp.path().join("pixi.toml"))
        .unwrap();
    original.write_all(b"secret = 'PRIVATE_TOKEN\n").unwrap();
    let output = project.command(&["exec", "/bin/true"]).output().unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_TOKEN"));
    assert!(fs::read_to_string(project.temp.path().join("pixi.toml"))
        .unwrap()
        .contains("PRIVATE_TOKEN"));
    project.assert_cleaned();
}
