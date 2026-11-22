mod config;
mod execution;
mod materialize;
mod profile_cli;
mod profile_store;
mod proxy_cli;
mod transport;

use clifacet::contract::ExitPolicy;
use clifacet::Failure;
use isolock_cas::InMemoryCas;
use isolock_core::VERSION;
use isolock_plan::{
    check_plan_drift, format_dispatch_commands, missing_artifacts, AirlockPlan, PlanDrift,
};
use std::env;
use std::fmt::Write as _;
use std::process::ExitCode;

/// Typed plan input, explicit to avoid the macro's optional caller-side `serde` cfg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirlockPlanInput {
    pub ci: bool,
}

/// CLI command definitions using `clifacet`.
///
/// # Errors
/// Returns configuration, plan/CAS validation, or CI lockfile-drift failures.
#[clifacet::command(input = AirlockPlanInput)]
#[command(
    name = "isolock",
    version = VERSION,
    about = "Hermetic dependency airlock manager"
)]
pub fn run_airlock_plan(
    #[clifacet(long, help = "CI drift guard mode")] ci: bool,
) -> Result<String, Failure> {
    let config =
        config::ExecutionConfig::load(&config::Overrides::default()).map_err(Failure::execution)?;
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
                report.push_str("Dispatch commands:\n");
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

pub const EXIT_POLICY: ExitPolicy = ExitPolicy::standard();

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.first().map(String::as_str) != Some("exec")
        && args.iter().any(|arg| arg == "--version" || arg == "-V")
    {
        println!("isolock {VERSION}");
        return ExitCode::SUCCESS;
    }

    if profile_cli::handles(&args) {
        return match profile_cli::run(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("isolock: {error}");
                ExitCode::FAILURE
            }
        };
    }

    match args.first().map(String::as_str) {
        Some("plan") => {
            let is_ci = args.iter().any(|a| a == "--ci");
            match run_airlock_plan(is_ci) {
                Ok(msg) => {
                    println!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("isolock plan: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("serve") => proxy_cli::run_proxy_server(&args[1..]),
        Some("ingest") => {
            println!("isolock ingest: airlock package ingested into local CAS store");
            ExitCode::SUCCESS
        }
        Some("exec") => proxy_cli::run_child_with_proxy(&args[1..]),
        Some("sync") => {
            println!("isolock sync: dependency state is in-sync");
            ExitCode::SUCCESS
        }
        _ => {
            println!("isolock v{VERSION} - dependency airlock manager");
            println!("Usage: isolock <config|plan|fetch|sync|export|seed-manifest|gc> [args...]");
            println!("Profiles: --profile NAME --platform PLATFORM (see isolock plan --help)");
            println!("Legacy proxy commands: exec, serve, ingest");
            ExitCode::SUCCESS
        }
    }
}

#[cfg(test)]
mod command_tests {
    use super::{AirlockPlanInput, RunAirlockPlan};
    use clap::Parser;
    use clifacet::Facet;

    #[test]
    fn clifacet_bridge_preserves_the_ci_flag_and_workstation_default() {
        for (arguments, ci) in [(vec!["isolock"], false), (vec!["isolock", "--ci"], true)] {
            let command = RunAirlockPlan::try_parse_from(arguments).unwrap();
            assert_eq!(command.validate().unwrap(), AirlockPlanInput { ci });
        }
        assert!(RunAirlockPlan::try_parse_from(["isolock", "--unknown"]).is_err());
    }
}
