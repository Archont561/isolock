//! Diffs the legacy `plan.json` against local CAS bytes behind the [`LegacyPlan`] port.

use crate::application::ports::{DriftState, LegacyPlan, SessionSettings, UseCaseResult};
use crate::domain::DomainError;
use crate::infrastructure::execution;
use isolock_cas::InMemoryCas;
use isolock_plan::{
    check_plan_drift, format_dispatch_commands, missing_artifacts, AirlockPlan, PlanDrift,
};

/// The reviewed legacy plan and the CAS directory it is checked against.
pub struct WorkspacePlan {
    session: SessionSettings,
}

impl WorkspacePlan {
    /// Binds the adapter to one resolved execution configuration projection.
    #[must_use]
    pub fn new(session: SessionSettings) -> Self {
        Self { session }
    }
}

impl LegacyPlan for WorkspacePlan {
    fn exists(&self) -> bool {
        self.session.plan.exists()
    }

    fn path(&self) -> String {
        self.session.plan.display().to_string()
    }

    fn drift(&self) -> UseCaseResult<DriftState> {
        let (expected, cas) = execution::inputs::load(&self.session).map_err(DomainError::new)?;
        let current = available_in_cas(&expected, &cas);
        match check_plan_drift(&expected, &current) {
            PlanDrift::InSync => Ok(DriftState::InSync),
            PlanDrift::DriftDetected {
                missing: missing_names,
                mismatched_digests,
            } => {
                let missing = missing_artifacts(&expected, &current);
                let commands = format_dispatch_commands(&missing).map_err(DomainError::new)?;
                let mut labels = Vec::new();
                if !missing_names.is_empty() {
                    labels = missing
                        .iter()
                        .map(|artifact| {
                            format!(
                                "{}=={} ({})",
                                artifact.name,
                                artifact.version,
                                artifact.registry.as_str()
                            )
                        })
                        .collect();
                }
                Ok(DriftState::Drift {
                    missing: labels,
                    mismatched_digests,
                    commands,
                })
            }
        }
    }
}

fn available_in_cas(expected: &AirlockPlan, cas: &InMemoryCas) -> AirlockPlan {
    let mut current = AirlockPlan::new();
    for (artifact, metadata) in expected.artifacts_with_metadata() {
        if cas.contains(&artifact.digest) {
            current.add_with_metadata(artifact.clone(), metadata.clone());
        }
    }
    current
}
