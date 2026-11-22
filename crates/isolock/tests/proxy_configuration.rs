//! Proxy operations share the Figment precedence/provenance boundary with profiles.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures fail fast on invalid setup.

mod coverage_support;

use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

struct Project {
    temp: TempDir,
}
impl Project {
    fn new() -> Self {
        Self {
            temp: tempfile::tempdir().unwrap(),
        }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .env_clear()
            .current_dir(self.temp.path())
            .args(args)
            .env("HOME", self.temp.path());
        coverage_support::preserve_profile(&mut command);
        command
    }
    fn explain(&self, args: &[&str]) -> Command {
        let mut command = self.command(&["config", "explain", "proxy.limits.max-concurrency"]);
        command.args(args);
        command
    }
}
fn output(output: Output) -> String {
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}
fn assert_source(text: &str, value: usize, source: &str) {
    assert!(
        text.contains(&format!("proxy.limits.max-concurrency = {value}\n")),
        "{text}"
    );
    assert!(text.contains(&format!("source: {source}")), "{text}");
}

#[test]
fn proxy_limits_follow_every_layer_and_report_the_winning_source() {
    let project = Project::new();
    assert_source(
        &output(project.explain(&[]).output().unwrap()),
        32,
        "compiled defaults",
    );
    let user = project.temp.path().join("user.toml");
    fs::write(&user, "[proxy.limits]\nmax-concurrency=3\n").unwrap();
    assert_source(
        &output(
            project
                .explain(&[])
                .env("ISOLOCK_USER_CONFIG", &user)
                .output()
                .unwrap(),
        ),
        3,
        "user operational configuration",
    );
    fs::write(
        project.temp.path().join("isolock.toml"),
        "[proxy.limits]\nmax-concurrency=4\n",
    )
    .unwrap();
    assert_source(
        &output(
            project
                .explain(&[])
                .env("ISOLOCK_USER_CONFIG", &user)
                .output()
                .unwrap(),
        ),
        4,
        "project configuration",
    );
    fs::create_dir(project.temp.path().join(".isolock")).unwrap();
    fs::write(
        project.temp.path().join(".isolock/config.local.toml"),
        "[proxy.limits]\nmax-concurrency=5\n",
    )
    .unwrap();
    assert_source(
        &output(project.explain(&[]).output().unwrap()),
        5,
        "generated project-local configuration",
    );
    assert_source(
        &output(
            project
                .explain(&[])
                .env("ISOLOCK_PROXY_MAX_CONCURRENCY", "6")
                .output()
                .unwrap(),
        ),
        6,
        "ISOLOCK_PROXY_MAX_CONCURRENCY",
    );
    assert_source(
        &output(
            project
                .explain(&["--max-concurrency", "7"])
                .env("ISOLOCK_PROXY_MAX_CONCURRENCY", "6")
                .output()
                .unwrap(),
        ),
        7,
        "explicit CLI flag",
    );
}

#[test]
fn unsafe_or_misspelled_proxy_limits_fail_closed_at_the_boundary() {
    let project = Project::new();
    for invalid in [
        "[proxy.limits]\nmax-concurrency=0",
        "[proxy.limits]\nmax-concurrency=257",
        "[proxy.limits]\nmax-concurency=2",
        "[proxy.limits]\nheader-timeout-ms=0",
        "[proxy.limits]\nwrite-timeout-ms=60001",
        "[proxy.limits]\nshutdown-grace-ms=0",
    ] {
        fs::write(project.temp.path().join("isolock.toml"), invalid).unwrap();
        assert!(!project
            .command(&["config", "check"])
            .output()
            .unwrap()
            .status
            .success());
        let output = project
            .command(&["exec", "missing-child"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            !project.temp.path().join(".isolock/cargo-home").exists(),
            "invalid limits must fail before session setup"
        );
    }
}

#[test]
fn user_defaults_may_set_operational_limits_but_not_weaken_proxy_mode() {
    let project = Project::new();
    let user = project.temp.path().join("user.toml");
    fs::write(&user, "[proxy]\nmode='connected'\n").unwrap();
    let result = project
        .command(&["config", "check"])
        .env("ISOLOCK_USER_CONFIG", &user)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("operational"));
}

#[test]
fn legacy_environment_aliases_are_typed_and_conflicting_aliases_are_rejected() {
    let project = Project::new();
    for (path, variable, value, expected) in [
        ("proxy.port", "ISOLOCK_PROXY_PORT", "4567", "4567"),
        ("proxy.plan", "ISOLOCK_PLAN", "my-plan.json", "my-plan.json"),
        (
            "proxy.cargo-home",
            "ISOLOCK_CARGO_HOME",
            "my-cargo",
            "my-cargo",
        ),
        ("proxy.mode", "ISOLOCK_MODE", "cI", "cI"),
    ] {
        let text = output(
            project
                .command(&["config", "explain", path])
                .env(variable, value)
                .output()
                .unwrap(),
        );
        assert!(text.contains(expected), "{text}");
        assert!(text.contains(&format!("source: {variable}")), "{text}");
    }
    let typed: serde_json::Value = serde_json::from_str(&output(
        project
            .command(&["config", "show"])
            .env("ISOLOCK_MODE", "cI")
            .output()
            .unwrap(),
    ))
    .unwrap();
    assert_eq!(typed["proxy"]["mode"], "ci");
    let result = project
        .command(&["config", "check"])
        .env("ISOLOCK_PROXY_MAX_CONCURRENCY", "3")
        .env("ISOLOCK_CONFIG_PROXY__LIMITS__MAX_CONCURRENCY", "4")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("same configuration key"));
    assert!(project
        .command(&["serve", "--help"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(!project
        .command(&["serve", "--port", "not-a-port"])
        .output()
        .unwrap()
        .status
        .success());
}
