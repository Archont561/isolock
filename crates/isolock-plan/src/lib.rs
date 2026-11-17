#![forbid(unsafe_code)]
//! Airlock plan generation, lockfile diffing, and drift detection.

use isolock_cas::Sha256Digest;
use isolock_core::Registry;

/// A single planned dependency artifact in the airlock manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedArtifact {
    pub name: String,
    pub version: String,
    pub registry: Registry,
    pub digest: Sha256Digest,
}

/// An immutable airlock plan containing locked artifacts for an environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AirlockPlan {
    artifacts: Vec<PlannedArtifact>,
}

impl AirlockPlan {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            artifacts: Vec::new(),
        }
    }

    pub fn add(&mut self, artifact: PlannedArtifact) {
        self.artifacts.push(artifact);
    }

    #[must_use]
    pub fn artifacts(&self) -> &[PlannedArtifact] {
        &self.artifacts
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }
}

/// Result of evaluating an airlock plan against lockfile state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanDrift {
    InSync,
    DriftDetected {
        missing: Vec<String>,
        mismatched_digests: Vec<String>,
    },
}

impl PlanDrift {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        matches!(self, Self::InSync)
    }
}

/// Detects drift between planned target dependencies and existing lockfile artifacts.
#[must_use]
pub fn check_plan_drift(expected: &AirlockPlan, current: &AirlockPlan) -> PlanDrift {
    let mut missing = Vec::new();
    let mut mismatched = Vec::new();

    for exp in expected.artifacts() {
        match current
            .artifacts()
            .iter()
            .find(|a| a.name == exp.name && a.version == exp.version)
        {
            None => missing.push(format!("{}@{}", exp.name, exp.version)),
            Some(curr) if curr.digest != exp.digest => {
                mismatched.push(format!("{}@{} (digest mismatch)", exp.name, exp.version));
            }
            Some(_) => {}
        }
    }

    if missing.is_empty() && mismatched.is_empty() {
        PlanDrift::InSync
    } else {
        PlanDrift::DriftDetected {
            missing,
            mismatched_digests: mismatched,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_drift_clean() {
        let digest =
            Sha256Digest::parse("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
                .unwrap();
        let artifact = PlannedArtifact {
            name: "numpy".into(),
            version: "2.1.0".into(),
            registry: Registry::Conda,
            digest,
        };

        let mut plan_a = AirlockPlan::new();
        plan_a.add(artifact.clone());

        let mut plan_b = AirlockPlan::new();
        plan_b.add(artifact);

        let drift = check_plan_drift(&plan_a, &plan_b);
        assert!(drift.is_clean());
    }
}
