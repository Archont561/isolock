//! Black-box contracts across the crates.io clifacet 0.10 command bridge.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Invalid fixtures must fail the test.

mod coverage_support;

use isolock_cas::Sha256Digest;
use isolock_core::Registry;
use isolock_plan::{AirlockPlan, PlannedArtifact};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;
use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

struct PlanCli {
    root: TempDir,
    output: RefCell<Option<Output>>,
}

#[fixture]
fn plan_cli() -> PlanCli {
    PlanCli {
        root: tempfile::tempdir().unwrap(),
        output: RefCell::new(None),
    }
}

impl PlanCli {
    fn write_plan(&self, plan: &AirlockPlan) {
        fs::create_dir_all(self.root.path().join(".isolock")).unwrap();
        fs::write(
            self.root.path().join(".isolock/plan.json"),
            serde_json::to_vec(plan).unwrap(),
        )
        .unwrap();
    }

    fn run(&self, ci: bool) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .current_dir(self.root.path())
            .env_clear()
            .env("HOME", self.root.path());
        coverage_support::preserve_profile(&mut command);
        command.arg("plan");
        if ci {
            command.arg("--ci");
        }
        *self.output.borrow_mut() = Some(command.output().unwrap());
    }
}

#[given("an isolated empty airlock plan")]
fn empty_plan(plan_cli: &PlanCli) {
    plan_cli.write_plan(&AirlockPlan::new());
}

#[given("an isolated plan requiring a missing Cargo archive")]
fn missing_artifact(plan_cli: &PlanCli) {
    let mut plan = AirlockPlan::new();
    plan.add(PlannedArtifact {
        registry: Registry::Cargo,
        name: "demo".into(),
        version: "1.0.0".into(),
        digest: Sha256Digest::from_bytes(b"expected archive"),
    });
    plan_cli.write_plan(&plan);
}

#[when("the plan is checked in CI mode")]
fn check_in_ci(plan_cli: &PlanCli) {
    plan_cli.run(true);
}

#[when("the plan is inspected in workstation mode")]
fn inspect(plan_cli: &PlanCli) {
    plan_cli.run(false);
}

#[then("the clean-plan verdict succeeds")]
fn clean(plan_cli: &PlanCli) {
    let output = plan_cli.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Verdict: ALL DEPENDENCIES SATISFIED"),
        "{stdout}"
    );
    assert!(stdout.contains("mode: ci"), "{stdout}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

fn drift(plan_cli: &PlanCli, ci: bool) {
    let output = plan_cli.output.borrow();
    let output = output.as_ref().unwrap();
    assert_eq!(output.status.code(), Some(i32::from(ci)), "{output:?}");
    let report = if ci {
        assert!(output.stdout.is_empty(), "{output:?}");
        String::from_utf8_lossy(&output.stderr)
    } else {
        assert!(output.stderr.is_empty(), "{output:?}");
        String::from_utf8_lossy(&output.stdout)
    };
    for expected in [
        "Verdict: DRIFT DETECTED",
        if ci { "mode: ci" } else { "mode: workstation" },
        "demo==1.0.0 (cargo)",
        "Review and commit isolock.ingest.json",
        "gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=cargo -f packages=\"demo==1.0.0\"",
    ] {
        assert!(report.contains(expected), "{report}");
    }
}

#[then("the drift verdict fails with an actionable dispatch command")]
fn ci_drift(plan_cli: &PlanCli) {
    drift(plan_cli, true);
}

#[then("the drift verdict succeeds with an actionable dispatch command")]
fn workstation_drift(plan_cli: &PlanCli) {
    drift(plan_cli, false);
}

#[scenario(
    path = "../../backlog/features/clifacet-compatibility.feature",
    name = "A clean airlock plan succeeds in CI"
)]
fn clean_ci_plan(plan_cli: PlanCli) {
    let _ = plan_cli;
}

#[scenario(
    path = "../../backlog/features/clifacet-compatibility.feature",
    name = "Missing locked artifacts fail closed in CI"
)]
fn ci_plan_detects_drift(plan_cli: PlanCli) {
    let _ = plan_cli;
}

#[scenario(
    path = "../../backlog/features/clifacet-compatibility.feature",
    name = "Workstation inspection reports drift without failing"
)]
fn workstation_plan_reports_drift(plan_cli: PlanCli) {
    let _ = plan_cli;
}
