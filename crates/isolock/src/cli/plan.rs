//! The legacy airlock plan is an operation, not an argument parser or entrypoint.

use crate::{config, execution};
use clifacet::Failure;
use isolock_cas::InMemoryCas;
use isolock_plan::{
    check_plan_drift, format_dispatch_commands, missing_artifacts, AirlockPlan, PlanDrift,
};
use std::fmt::Write as _;

pub(super) fn run(ci: bool, overrides: &config::Overrides) -> Result<String, Failure> {
    let config = config::ExecutionConfig::load(overrides).map_err(Failure::execution)?;
    if !config.plan.exists() {
        return Err(Failure::execution(format!(
            "cannot evaluate lockfile drift: no airlock plan found at {}",
            config.plan.display()
        )));
    }
    let (expected, cas) = execution::inputs::load(&config).map_err(Failure::execution)?;
    let current = plan_available_in_cas(&expected, &cas);
    match check_plan_drift(&expected, &current) {
        PlanDrift::InSync => Ok(format!(
            "Verdict: ALL DEPENDENCIES SATISFIED\nisolock plan [mode: {}]: lockfile is clean and in-sync with CAS",
            if ci { "ci" } else { "workstation" }
        )),
        PlanDrift::DriftDetected { missing: missing_names, mismatched_digests } => {
            let missing = missing_artifacts(&expected, &current);
            let commands = format_dispatch_commands(&missing).map_err(Failure::execution)?;
            let mut report = format!(
                "Verdict: DRIFT DETECTED\nPlan mode: {}\n",
                if ci { "ci" } else { "workstation" }
            );
            if !missing_names.is_empty() {
                report.push_str("Missing packages:\n");
                for artifact in missing {
                    let _ = writeln!(report,
                        "- {}=={} ({})",
                        artifact.name,
                        artifact.version,
                        artifact.registry.as_str()
                    );
                }
            }
            if !mismatched_digests.is_empty() {
                report.push_str("Mismatched digests:\n");
                for artifact in mismatched_digests {
                    let _ = writeln!(report, "- {artifact}");
                }
            }
            if !commands.is_empty() {
                report.push_str("Review and commit isolock.ingest.json with exact source, filename, and SHA-256 pins before dispatch. No registry resolution is performed.\nDispatch commands:\n");
                for command in commands {
                    let _ = writeln!(report, "{command}");
                }
            }
            let report = report.trim_end().to_owned();
            if ci {
                Err(Failure::execution(report))
            } else {
                Ok(report)
            }
        }
    }
}

fn plan_available_in_cas(expected: &AirlockPlan, cas: &InMemoryCas) -> AirlockPlan {
    let mut current = AirlockPlan::new();
    for (artifact, metadata) in expected.artifacts_with_metadata() {
        if cas.contains(&artifact.digest) {
            current.add_with_metadata(artifact.clone(), metadata.clone());
        }
    }
    current
}
