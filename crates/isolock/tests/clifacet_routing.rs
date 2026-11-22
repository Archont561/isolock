//! The real process is the contract for clifacet parsing, dispatch and streams.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid fixtures must fail the test.

mod coverage_support;

use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;
use std::process::{Command, Output};
use tempfile::TempDir;

struct CliWorld {
    root: TempDir,
    output: RefCell<Option<Output>>,
}

#[fixture]
fn world() -> CliWorld {
    CliWorld {
        root: tempfile::tempdir().unwrap(),
        output: RefCell::new(None),
    }
}

impl CliWorld {
    fn run(&self, arguments: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .env_clear()
            .env("HOME", self.root.path())
            .current_dir(self.root.path())
            .args(arguments);
        coverage_support::preserve_profile(&mut command);
        command.output().unwrap()
    }
}

#[given("an isolated CLI workspace")]
fn isolated_workspace(world: &CliWorld) {
    assert!(!world.root.path().join(".isolock").exists());
}

#[when("an unknown command is requested")]
fn unknown_command(world: &CliWorld) {
    *world.output.borrow_mut() = Some(world.run(&["unknown-command"]));
}

#[given("project configuration is invalid")]
fn invalid_configuration(world: &CliWorld) {
    std::fs::write(world.root.path().join("isolock.toml"), "invalid TOML [").unwrap();
}

#[when("no command is requested")]
fn no_command(world: &CliWorld) {
    *world.output.borrow_mut() = Some(world.run(&[]));
}

#[when("CLI arguments {arguments:string} are requested")]
fn run_arguments(world: &CliWorld, arguments: String) {
    let arguments = arguments.split_whitespace().collect::<Vec<_>>();
    *world.output.borrow_mut() = Some(world.run(&arguments));
}

#[then("a usage error mentioning {diagnostic:string} is reported on stderr")]
fn diagnostic(world: &CliWorld, diagnostic: String) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&diagnostic), "{stderr}");
    assert!(!stderr.contains("invalid configuration"), "{stderr}");
}

#[then("a usage error is reported on stderr")]
fn usage_error(world: &CliWorld) {
    diagnostic(world, "unrecognized subcommand".into());
    let output = world.output.borrow();
    let stderr = String::from_utf8_lossy(&output.as_ref().unwrap().stderr);
    assert!(stderr.contains("Usage:"), "{stderr}");
}

#[then("generated help is reported on stdout")]
fn help(world: &CliWorld) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: isolock"), "{stdout}");
    assert!(stdout.contains("--help"), "{stdout}");
    assert!(stdout.contains("--profile"), "{stdout}");
}

#[then("no application state is created")]
fn no_state(world: &CliWorld) {
    assert!(!world.root.path().join(".isolock").exists());
}

#[when("a child receives application-looking arguments {separator:string}")]
fn child_arguments(world: &CliWorld, separator: String) {
    let mut arguments = vec!["exec"];
    match separator.as_str() {
        "after --" => arguments.push("--"),
        "directly" => (),
        _ => panic!("unexpected separator fixture: {separator}"),
    }
    arguments.extend([
        "/bin/sh",
        "-c",
        r#"printf '%s\000' "$@""#,
        "child",
        "--version",
        "--help",
        "--profile",
        "child-profile",
        "--config",
        "child.toml",
        "--ci",
        "--port",
        "99",
        "--",
        "",
        "two words",
    ]);
    *world.output.borrow_mut() = Some(world.run(&arguments));
}

#[then("its complete argument tail is passed through unchanged")]
fn complete_tail(world: &CliWorld) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(output.stdout,
        b"--version\0--help\0--profile\0child-profile\0--config\0child.toml\0--ci\0--port\x0099\0--\0\0two words\0");
}

#[when("a child exits with status {status:u8}")]
fn child_status(world: &CliWorld, status: u8) {
    let script = format!("printf 'child stdout'; printf 'child stderr' >&2; exit {status}");
    *world.output.borrow_mut() = Some(world.run(&["exec", "/bin/sh", "-c", &script]));
}

#[then("exit status {status:u8} and both child streams are preserved")]
fn child_streams(world: &CliWorld, status: u8) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(i32::from(status)), "{output:?}");
    assert_eq!(output.stdout, b"child stdout");
    assert_eq!(output.stderr, b"child stderr");
}

#[then("the nested command reports the explicit proxy port")]
fn nested_overrides(world: &CliWorld) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"proxy.port = 8123\nsource: explicit CLI flag\n"
    );
}

#[then("ingestion reports that the reviewed manifest is missing")]
fn ingestion(world: &CliWorld) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot read reviewed ingestion manifest")
    );
}

#[then("the application version succeeds on stdout")]
fn version(world: &CliWorld) {
    let output = world.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("isolock {}\n", isolock_core::VERSION)
    );
}

#[scenario(
    path = "../../backlog/features/clifacet-routing.feature",
    name = "Unknown commands fail before application setup"
)]
fn unknown_commands_are_usage_errors(world: CliWorld) {
    let _ = world;
}

#[scenario(
    path = "../../backlog/features/clifacet-routing.feature",
    name = "A missing command shows root usage"
)]
fn missing_command_shows_usage(world: CliWorld) {
    let _ = world;
}

#[scenario(
    path = "../../backlog/features/clifacet-routing.feature",
    name = "Invalid arguments fail before application setup"
)]
fn invalid_arguments_do_not_initialize_the_application(
    world: CliWorld,
    arguments: String,
    diagnostic: String,
) {
    let _ = (world, arguments, diagnostic);
}

#[scenario(
    path = "../../backlog/features/clifacet-routing.feature",
    name = "Help does not initialize the application"
)]
fn help_does_not_initialize_the_application(world: CliWorld, arguments: String) {
    let _ = (world, arguments);
}

#[cfg(unix)]
#[scenario(
    path = "../../backlog/features/clifacet-routing.feature",
    name = "Child arguments are opaque to the application parser"
)]
fn child_arguments_are_opaque(world: CliWorld, separator: String) {
    let _ = (world, separator);
}

#[cfg(unix)]
#[scenario(
    path = "../../backlog/features/clifacet-routing.feature",
    name = "Child exit statuses are not collapsed into application errors"
)]
fn child_statuses_are_preserved(world: CliWorld, status: u8) {
    let _ = (world, status);
}

#[scenario(
    path = "../../backlog/features/clifacet-routing.feature",
    name = "Global overrides reach nested commands"
)]
fn globals_reach_nested_commands(world: CliWorld, arguments: String) {
    let _ = (world, arguments);
}

#[scenario(
    path = "../../backlog/features/clifacet-routing.feature",
    name = "Ingestion requires reviewed pins instead of placeholder success"
)]
fn ingestion_requires_a_review(world: CliWorld) {
    let _ = world;
}

#[scenario(
    path = "../../backlog/features/clifacet-routing.feature",
    name = "Version does not initialize the application"
)]
fn version_does_not_initialize_the_application(world: CliWorld) {
    let _ = world;
}
