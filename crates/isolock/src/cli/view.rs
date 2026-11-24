//! Renders application outcomes as the exact text an operator already expects.
//!
//! Every string here is a compatibility contract: the CLI contract tests assert these lines.

use crate::application::ports::CollectionReport;
use crate::application::use_cases::{
    ConfigurationOutcome, ExecOutcome, FetchOutcome, PlanOutcome, SeedOutcome, SyncOutcome,
};
use std::fmt::Write as _;

/// Renders a configuration outcome.
#[must_use]
pub fn configuration(outcome: &ConfigurationOutcome) -> String {
    outcome.text.clone()
}

/// Renders a plan outcome.
///
/// The returned string is what the CLI prints; a drift report in CI mode is returned as an
/// error by the caller.
#[must_use]
pub fn plan(outcome: &PlanOutcome, ci: bool) -> String {
    match outcome {
        PlanOutcome::Planned {
            profile,
            platform,
            digest,
            alias,
            missing,
        } => {
            let mut text = format!("Profile {profile}/{platform}: {digest}\nplan: {alias}");
            if *missing > 0 {
                // Writing into the buffer avoids a second, discarded allocation.
                let _ = write!(
                    text,
                    "\n{missing} selected artifact(s) are not yet in CAS; use fetch or ingest the reviewed originals"
                );
            }
            text
        }
        PlanOutcome::Checked { profile, platform } => {
            format!("Profile {profile}/{platform}: locked inputs match; ALL DEPENDENCIES SATISFIED")
        }
        PlanOutcome::AirlockInSync => format!(
            "Verdict: ALL DEPENDENCIES SATISFIED\nisolock plan [mode: {}]: lockfile is clean and in-sync with CAS",
            if ci { "ci" } else { "workstation" }
        ),
        PlanOutcome::AirlockDrift { text } => format!(
            "Verdict: DRIFT DETECTED\nPlan mode: {}\n{text}",
            if ci { "ci" } else { "workstation" }
        ),
    }
}

/// Whether the rendered plan outcome must fail the command in CI mode.
#[must_use]
pub fn plan_fails_ci(outcome: &PlanOutcome, ci: bool) -> bool {
    ci && matches!(outcome, PlanOutcome::AirlockDrift { .. })
}

/// Renders a fetch outcome.
#[must_use]
pub fn fetch(outcome: &FetchOutcome) -> String {
    format!(
        "Profile {}/{}: selected closure is verified in CAS",
        outcome.profile, outcome.platform
    )
}

/// Renders a sync outcome.
#[must_use]
pub fn sync(outcome: &SyncOutcome) -> String {
    format!(
        "Profile {}/{} ready: {}\nActivate: . .isolock/activate/{}-{}",
        outcome.profile, outcome.platform, outcome.prefix, outcome.profile, outcome.platform
    )
}

/// Renders an export outcome.
#[must_use]
pub fn export(profile: &str, platform: &str) -> String {
    format!("Exported selected profile {profile}/{platform}")
}

/// Renders a seed-manifest outcome.
#[must_use]
pub fn seed(outcome: &SeedOutcome) -> String {
    format!("POSIX bootstrap manifest: {}", outcome.manifest)
}

/// Renders a collection report.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when the report cannot be serialized.
pub fn collection(report: &CollectionReport) -> crate::application::UseCaseResult<String> {
    report.render()
}

/// Renders the child status warning line, if the session reported one.
#[must_use]
pub fn shutdown_warning(outcome: &ExecOutcome) -> Option<String> {
    outcome
        .shutdown_warning
        .as_ref()
        .map(|warning| warning.line())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::use_cases::PlanOutcome;

    #[test]
    fn a_fresh_plan_reports_digest_alias_and_missing_count() {
        let text = plan(
            &PlanOutcome::Planned {
                profile: "dev".into(),
                platform: "linux-64".into(),
                digest: "abc".into(),
                alias: ".isolock/plans/dev-linux-64.lock".into(),
                missing: 2,
            },
            false,
        );
        assert_eq!(
            text,
            "Profile dev/linux-64: abc\nplan: .isolock/plans/dev-linux-64.lock\n2 selected artifact(s) are not yet in CAS; use fetch or ingest the reviewed originals"
        );
    }

    #[test]
    fn a_clean_plan_has_no_missing_artifact_line() {
        let text = plan(
            &PlanOutcome::Planned {
                profile: "dev".into(),
                platform: "linux-64".into(),
                digest: "abc".into(),
                alias: "plan".into(),
                missing: 0,
            },
            false,
        );
        assert!(!text.contains("not yet in CAS"));
    }

    #[test]
    fn the_legacy_verdict_reports_the_mode() {
        let ci = plan(&PlanOutcome::AirlockInSync, true);
        assert!(ci.contains("[mode: ci]"));
        assert!(ci.contains("Verdict: ALL DEPENDENCIES SATISFIED"));
        let local = plan(&PlanOutcome::AirlockInSync, false);
        assert!(local.contains("[mode: workstation]"));
    }

    #[test]
    fn drift_reports_the_mode_and_fails_only_in_ci() {
        let outcome = PlanOutcome::AirlockDrift {
            text: "Missing packages:\n- demo==1.0.0 (cargo)".into(),
        };
        assert!(
            plan(&outcome, false).starts_with("Verdict: DRIFT DETECTED\nPlan mode: workstation")
        );
        assert!(!plan_fails_ci(&outcome, false));
        assert!(plan_fails_ci(&outcome, true));
    }

    #[test]
    fn a_collection_report_renders_the_reviewed_json_document() {
        let text = collection(&CollectionReport {
            roots: vec!["dev".into()],
            retained: vec!["aa".into()],
            reclaimable: vec![],
            bytes: 0,
            applied: false,
        })
        .expect("serializable");
        assert!(text.contains("\"roots\": ["));
        assert!(text.contains("\"applied\": false"));
    }

    #[test]
    fn sync_reports_the_activation_command() {
        let text = sync(&SyncOutcome {
            profile: "dev".into(),
            platform: "linux-64".into(),
            prefix: "/tmp/prefix".into(),
            transferred: 0,
        });
        assert_eq!(
            text,
            "Profile dev/linux-64 ready: /tmp/prefix\nActivate: . .isolock/activate/dev-linux-64"
        );
    }
}
