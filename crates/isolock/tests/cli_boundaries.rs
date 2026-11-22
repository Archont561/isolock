//! Public CLI failures must be diagnostic, fail closed, and leave no partial execution.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Fixture setup must fail the test.

mod coverage_support;

use isolock_cas::Sha256Digest;
use isolock_core::Registry;
use isolock_plan::{AirlockPlan, PlannedArtifact};
use std::fs;
use std::net::TcpListener;
use std::process::{Command, Output};

struct Project(tempfile::TempDir);
impl Project {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }
    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .args(arguments)
            .current_dir(self.0.path())
            .env_clear()
            .env("HOME", self.0.path())
            .env("PATH", "/usr/bin:/bin");
        coverage_support::preserve_profile(&mut command);
        command
    }
    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().unwrap()
    }
    fn config(&self, contents: &str) {
        fs::write(self.0.path().join("isolock.toml"), contents).unwrap();
    }
    fn plan(&self) -> Sha256Digest {
        let digest = Sha256Digest::from_bytes(b"expected archive");
        let mut plan = AirlockPlan::new();
        plan.add(PlannedArtifact {
            registry: Registry::Cargo,
            name: "demo".into(),
            version: "1.0.0".into(),
            digest: digest.clone(),
        });
        fs::create_dir_all(self.0.path().join(".isolock")).unwrap();
        fs::write(
            self.0.path().join(".isolock/plan.json"),
            serde_json::to_vec(&plan).unwrap(),
        )
        .unwrap();
        digest
    }
}

fn failure(output: Output, expected: &str) {
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "{output:?}"
    );
}

#[test]
fn configuration_validation_rejects_invalid_schema_paths_references_and_modes() {
    let project = Project::new();
    for (config, expected) in [
        ("schema-version=2", "unsupported configuration schema"),
        ("[storage]\ncas-dir=''", "cas-dir cannot be empty"),
        ("[storage]\nreference=''", "invalid storage reference"),
        (
            "[storage]\nreference='-unsafe'",
            "invalid storage reference",
        ),
        (
            "[storage]\nsource='https://secret@example.invalid/repo'",
            "credentials",
        ),
        ("[proxy]\nplan=''", "paths cannot be empty"),
        ("[proxy]\ncargo-home=''", "paths cannot be empty"),
        ("[proxy]\nmode='unsupported'", "invalid configuration"),
        ("[profiles.empty]", "must declare platforms"),
    ] {
        project.config(config);
        failure(project.run(&["config", "check"]), expected);
        assert!(!project.0.path().join(".isolock/cargo-home").exists());
    }
}

#[test]
fn user_config_errors_are_redacted_and_implicit_xdg_configuration_is_loaded() {
    let project = Project::new();
    let user = project.0.path().join("user.toml");
    fs::write(&user, "secret='PRIVATE_TOKEN\n").unwrap();
    let output = project
        .command(&["config", "check"])
        .env("ISOLOCK_USER_CONFIG", &user)
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_TOKEN"));
    failure(output, "invalid user operational configuration");
    fs::write(&user, "[storage]\nreference='unreviewed'\n").unwrap();
    failure(
        project
            .command(&["config", "check"])
            .env("ISOLOCK_USER_CONFIG", &user)
            .output()
            .unwrap(),
        "only set operational",
    );
    let xdg = project.0.path().join("xdg");
    fs::create_dir_all(xdg.join("isolock")).unwrap();
    fs::write(xdg.join("isolock/config.toml"), "[proxy]\nport=4321\n").unwrap();
    let output = project
        .command(&["config", "explain", "proxy.port"])
        .env("XDG_CONFIG_HOME", xdg)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("proxy.port = 4321"));
    assert!(text.contains("source: user operational configuration"));
}

#[test]
fn explicit_missing_configuration_and_unscoped_profile_overrides_are_errors() {
    let project = Project::new();
    failure(
        project.run(&["config", "check", "--config", "absent.toml"]),
        "explicit project configuration",
    );
    failure(
        project.run(&["config", "explain", "missing.path"]),
        "unknown configuration path",
    );
    for flag in ["--pixi-environment", "--pixi-version"] {
        failure(
            project.run(&["config", "check", flag, "value"]),
            "require --profile",
        );
    }
    for variable in ["ISOLOCK_PIXI_ENVIRONMENT", "ISOLOCK_PIXI_VERSION"] {
        failure(
            project
                .command(&["config", "check"])
                .env(variable, "value")
                .output()
                .unwrap(),
            "require --profile",
        );
    }
    for mode in ["connected", "ci", "localairlock"] {
        let output = project
            .command(&["config", "show"])
            .env("ISOLOCK_MODE", mode)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let output = project.run(&["config", "explain", "proxy.mode", "--mode", "connected"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("explicit CLI flag"));
}

#[test]
fn serve_rejects_missing_invalid_and_unreadable_plan_inputs_without_listening() {
    let project = Project::new();
    failure(
        project.run(&["serve", "--unknown-flag"]),
        "unexpected argument",
    );
    failure(
        project
            .command(&["serve"])
            .env("ISOLOCK_PLAN", "absent.json")
            .output()
            .unwrap(),
        "cannot read plan",
    );
    fs::write(project.0.path().join("invalid.json"), b"invalid JSON").unwrap();
    failure(
        project
            .command(&["serve"])
            .env("ISOLOCK_PLAN", "invalid.json")
            .output()
            .unwrap(),
        "invalid plan",
    );
    project.plan();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    failure(
        project.run(&["serve", "--port", &port]),
        "cannot bind loopback listener",
    );
    assert!(
        listener.local_addr().is_ok(),
        "the conflicting listener must remain owned by its caller"
    );
}

#[test]
fn corrupt_or_non_file_cas_entries_fail_before_serving_artifact_bytes() {
    let project = Project::new();
    let digest = project.plan();
    let path = project
        .0
        .path()
        .join(".isolock/cas")
        .join(digest.sharded_path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"corrupt archive").unwrap();
    failure(project.run(&["serve"]), "invalid CAS blob");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    failure(project.run(&["serve"]), "cannot read CAS blob");
}

#[test]
fn legacy_plan_reports_actionable_drift_and_ci_returns_failure() {
    let project = Project::new();
    project.plan();
    let output = project.run(&["plan"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "Verdict: DRIFT DETECTED",
        "mode: workstation",
        "demo==1.0.0 (cargo)",
        "gh workflow run",
    ] {
        assert!(stdout.contains(expected), "{stdout}");
    }
    failure(project.run(&["plan", "--ci"]), "DRIFT DETECTED");
}

#[cfg(unix)]
#[test]
fn execution_reports_port_and_config_write_failures_without_starting_children() {
    let project = Project::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    failure(
        project
            .command(&["exec", "/bin/true"])
            .env("ISOLOCK_MODE", "connected")
            .env(
                "ISOLOCK_PROXY_PORT",
                listener.local_addr().unwrap().port().to_string(),
            )
            .output()
            .unwrap(),
        "cannot start local proxy",
    );
    fs::create_dir_all(project.0.path().join(".isolock/cargo-home/config.toml")).unwrap();
    failure(
        project.run(&["exec", "/bin/true"]),
        "cannot write Cargo config",
    );
    failure(project.run(&["exec"]), "expected a command to run");
}
