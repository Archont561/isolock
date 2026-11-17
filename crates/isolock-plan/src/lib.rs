#![forbid(unsafe_code)]
//! Airlock plan generation, lockfile diffing, and drift detection.

use isolock_cas::Sha256Digest;
use isolock_core::Registry;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};

/// A single planned dependency artifact in the airlock manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedArtifact {
    pub name: String,
    pub version: String,
    pub registry: Registry,
    pub digest: Sha256Digest,
}

/// Optional registry metadata captured while an artifact is ingested.
///
/// Supplying the original filename is recommended because registry archives do
/// not always encode every filename detail in their contents. Conda metadata may
/// contain either the archive's `info/index.json` object or a complete
/// `PackageRecord`; other registries may use this field for future index data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlannedArtifactMetadata {
    /// Original archive filename, including its extension.
    pub filename: Option<String>,
    /// Conda channel subdirectory such as `linux-64` or `noarch`.
    pub subdir: Option<String>,
    /// Registry-specific metadata captured by the ingest pipeline.
    pub registry_metadata: Option<serde_json::Value>,
    /// `Requires-Python` constraint for a PyPI wheel, if already known.
    pub requires_python: Option<String>,
}

/// An immutable airlock plan containing locked artifacts for an environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AirlockPlan {
    artifacts: Vec<PlannedArtifact>,
    artifact_metadata: Vec<PlannedArtifactMetadata>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SerializedAirlockPlan {
    artifacts: Vec<PlannedArtifact>,
    artifact_metadata: Vec<PlannedArtifactMetadata>,
}

impl<'de> Deserialize<'de> for AirlockPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut serialized = SerializedAirlockPlan::deserialize(deserializer)?;
        if serialized.artifact_metadata.len() > serialized.artifacts.len() {
            return Err(D::Error::custom(
                "airlock plan contains metadata without a corresponding artifact",
            ));
        }
        serialized
            .artifact_metadata
            .resize_with(serialized.artifacts.len(), PlannedArtifactMetadata::default);
        Ok(Self {
            artifacts: serialized.artifacts,
            artifact_metadata: serialized.artifact_metadata,
        })
    }
}

impl AirlockPlan {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            artifacts: Vec::new(),
            artifact_metadata: Vec::new(),
        }
    }

    pub fn add(&mut self, artifact: PlannedArtifact) {
        self.add_with_metadata(artifact, PlannedArtifactMetadata::default());
    }

    /// Adds an artifact with registry-specific details retained from ingestion.
    pub fn add_with_metadata(
        &mut self,
        artifact: PlannedArtifact,
        metadata: PlannedArtifactMetadata,
    ) {
        self.artifacts.push(artifact);
        self.artifact_metadata.push(metadata);
    }

    /// Returns all planned artifacts in insertion order.
    #[must_use]
    pub fn artifacts(&self) -> &[PlannedArtifact] {
        &self.artifacts
    }

    /// Returns metadata associated with an artifact index.
    #[must_use]
    pub fn artifact_metadata(&self, index: usize) -> Option<&PlannedArtifactMetadata> {
        self.artifact_metadata.get(index)
    }

    /// Iterates over each artifact and its retained registry metadata.
    pub fn artifacts_with_metadata(
        &self,
    ) -> impl Iterator<Item = (&PlannedArtifact, &PlannedArtifactMetadata)> {
        self.artifacts.iter().zip(&self.artifact_metadata)
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

    #[test]
    fn plan_metadata_roundtrips_and_legacy_artifacts_get_empty_metadata() {
        let legacy = r#"{"artifacts":[{"name":"requests","version":"2.32.3","registry":"Pypi","digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}]}"#;
        let mut plan: AirlockPlan = serde_json::from_str(legacy).expect("legacy plan parses");
        assert_eq!(
            plan.artifact_metadata(0),
            Some(&PlannedArtifactMetadata::default())
        );

        plan.add_with_metadata(
            PlannedArtifact {
                name: "numpy".into(),
                version: "2.1.0".into(),
                registry: Registry::Conda,
                digest: Sha256Digest::from_bytes(b"archive"),
            },
            PlannedArtifactMetadata {
                filename: Some("numpy-2.1.0-py311.conda".into()),
                subdir: Some("linux-64".into()),
                registry_metadata: Some(serde_json::json!({"depends": ["python >=3.11"]})),
                requires_python: None,
            },
        );
        let serialized = serde_json::to_string(&plan).expect("plan serializes");
        let restored: AirlockPlan = serde_json::from_str(&serialized).expect("plan restores");
        assert_eq!(restored, plan);
        assert_eq!(restored.artifacts_with_metadata().count(), 2);
    }
}
