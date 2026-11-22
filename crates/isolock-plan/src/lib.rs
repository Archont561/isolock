#![forbid(unsafe_code)]
//! Airlock plan generation, lockfile diffing, and drift detection.

pub mod profile;

use isolock_cas::Sha256Digest;
use isolock_core::Registry;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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

const WORKFLOW_REGISTRY_OPTIONS: &[&str] = &["cargo", "conda", "pypi", "npm"];

/// Returns the expected artifacts that have no matching registry/name/version in `current`.
#[must_use]
pub fn missing_artifacts(expected: &AirlockPlan, current: &AirlockPlan) -> Vec<PlannedArtifact> {
    expected
        .artifacts()
        .iter()
        .filter(|expected_artifact| {
            !current.artifacts().iter().any(|current_artifact| {
                current_artifact.name == expected_artifact.name
                    && current_artifact.version == expected_artifact.version
                    && current_artifact.registry == expected_artifact.registry
            })
        })
        .cloned()
        .collect()
}

/// Formats registry-grouped GitHub Actions dispatch commands for missing artifacts.
///
/// Commands explicitly select the reviewed `isolock.ingest.json` manifest; they do not
/// resolve registries or synthesize trusted sources/digests. Package specifications use
/// shell-safe characters for copy/paste; the workflow also passes them as literal argv.
/// Registry names match the choices declared by `.github/workflows/isolock.yml`.
///
/// # Errors
/// Returns an error for an unsupported registry or package/version that cannot safely
/// be passed as workflow dispatch input.
pub fn format_dispatch_commands(missing: &[PlannedArtifact]) -> Result<Vec<String>, String> {
    let mut grouped: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();

    for artifact in missing {
        let registry = artifact.registry.as_str();
        if !WORKFLOW_REGISTRY_OPTIONS.contains(&registry) {
            return Err(format!(
                "registry `{registry}` is not accepted by isolock.yml workflow_dispatch"
            ));
        }
        if !is_safe_package_name(artifact.registry, &artifact.name)
            || !is_safe_version(&artifact.version)
        {
            return Err(format!(
                "cannot safely pass package `{}=={}` to isolock.yml workflow_dispatch",
                artifact.name, artifact.version
            ));
        }

        grouped
            .entry(registry)
            .or_default()
            .insert(format!("{}=={}", artifact.name, artifact.version));
    }

    let mut commands = Vec::with_capacity(grouped.len());
    for registry in WORKFLOW_REGISTRY_OPTIONS {
        if let Some(packages) = grouped.remove(registry) {
            if packages.is_empty() {
                continue;
            }
            let packages = packages.into_iter().collect::<Vec<_>>().join(" ");
            commands.push(format!(
                "gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry={registry} -f packages=\"{packages}\""
            ));
        }
    }
    if let Some((registry, _)) = grouped.into_iter().next() {
        return Err(format!(
            "registry `{registry}` is not accepted by isolock.yml workflow_dispatch"
        ));
    }
    Ok(commands)
}

fn is_safe_package_name(registry: Registry, value: &str) -> bool {
    if registry == Registry::Npm {
        if let Some((scope, name)) = value
            .strip_prefix('@')
            .and_then(|rest| rest.split_once('/'))
        {
            return is_safe_identifier(scope) && is_safe_identifier(name);
        }
    }
    is_safe_identifier(value)
}

fn is_safe_identifier(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn is_safe_version(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
}

/// Detects drift between planned target dependencies and existing lockfile artifacts.
#[must_use]
pub fn check_plan_drift(expected: &AirlockPlan, current: &AirlockPlan) -> PlanDrift {
    let missing = missing_artifacts(expected, current)
        .iter()
        .map(|artifact| format!("{}@{}", artifact.name, artifact.version))
        .collect::<Vec<_>>();
    let mut mismatched = Vec::new();

    for exp in expected.artifacts() {
        if let Some(curr) = current.artifacts().iter().find(|artifact| {
            artifact.name == exp.name
                && artifact.version == exp.version
                && artifact.registry == exp.registry
        }) {
            if curr.digest != exp.digest {
                mismatched.push(format!("{}@{} (digest mismatch)", exp.name, exp.version));
            }
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

    fn test_artifact(name: &str, version: &str, registry: Registry) -> PlannedArtifact {
        PlannedArtifact {
            name: name.into(),
            version: version.into(),
            registry,
            digest: Sha256Digest::from_bytes(format!("{name}-{version}").as_bytes()),
        }
    }

    #[test]
    fn drift_matching_includes_the_registry() {
        let mut expected = AirlockPlan::new();
        expected.add(test_artifact("numpy", "2.1.0", Registry::Pypi));
        expected.add(test_artifact("numpy", "2.1.0", Registry::Conda));

        let mut current = AirlockPlan::new();
        current.add(test_artifact("numpy", "2.1.0", Registry::Conda));

        assert_eq!(
            missing_artifacts(&expected, &current),
            vec![test_artifact("numpy", "2.1.0", Registry::Pypi)]
        );
        assert_eq!(
            check_plan_drift(&expected, &current),
            PlanDrift::DriftDetected {
                missing: vec!["numpy@2.1.0".into()],
                mismatched_digests: Vec::new(),
            }
        );
    }

    #[test]
    fn dispatch_commands_group_and_sort_packages_by_registry() {
        let missing = vec![
            test_artifact("pydantic", "2.9.2", Registry::Pypi),
            test_artifact("serde", "1.0.210", Registry::Cargo),
            test_artifact("fastapi", "0.115.0", Registry::Pypi),
            test_artifact("fastapi", "0.115.0", Registry::Pypi),
        ];

        assert_eq!(
            format_dispatch_commands(&missing).expect("package inputs are safe"),
            vec![
                "gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=cargo -f packages=\"serde==1.0.210\"",
                "gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=pypi -f packages=\"fastapi==0.115.0 pydantic==2.9.2\"",
            ]
        );
    }

    #[test]
    fn dispatch_commands_reject_shell_unsafe_package_values() {
        let missing = vec![test_artifact("fastapi;touch", "0.115.0", Registry::Pypi)];
        assert!(format_dispatch_commands(&missing)
            .expect_err("unsafe package values cannot be emitted")
            .contains("cannot safely pass package"));
    }

    #[test]
    fn dispatch_commands_support_scoped_npm_names() {
        let missing = vec![test_artifact("@types/node", "22.5.5", Registry::Npm)];
        assert_eq!(
            format_dispatch_commands(&missing).expect("scoped package input is safe"),
            vec!["gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=npm -f packages=\"@types/node==22.5.5\""]
        );
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
