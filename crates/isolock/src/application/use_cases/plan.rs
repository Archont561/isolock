//! Planning a profile and checking a locked graph, plus the legacy airlock drift report.

use crate::application::ports::{
    DriftState, LegacyPlan, PlanStore, ProfileGraph, UseCaseResult, WorkspaceSelection,
};
use crate::domain::{DomainError, Settings};
use isolock_plan::profile::{build_profiles, identifier, PlanRequest, ProfileManifest};

/// What a plan command decided, rendered by the CLI adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanOutcome {
    /// A fresh plan was built and published.
    Planned {
        /// The selected profile name.
        profile: String,
        /// The selected platform.
        platform: String,
        /// The digest of the selected profile's manifest.
        digest: String,
        /// The written alias path, as the operator sees it.
        alias: String,
        /// How many selected artifacts are still absent from CAS.
        missing: usize,
    },
    /// A locked graph was checked and every input matches.
    Checked {
        /// The selected profile name.
        profile: String,
        /// The selected platform.
        platform: String,
    },
    /// The legacy airlock plan is satisfied.
    AirlockInSync,
    /// The legacy airlock plan drifted; `text` is the report the operator must read.
    AirlockDrift {
        /// The full report, including dispatch commands.
        text: String,
    },
}

/// Builds and publishes a fresh plan for the selected profile and platform.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when an identifier is invalid, a locked input
/// cannot be produced, or the plan cannot be written.
pub fn plan_profile<S: PlanStore>(
    store: &S,
    settings: &Settings,
    selection: &WorkspaceSelection,
) -> UseCaseResult<PlanOutcome> {
    let profile = selection.require_profile()?;
    let platform = selection.require_platform()?;
    identifier(profile)?;
    identifier(platform)?;
    let _lease = store.lease()?;
    let identity = store.storage_identity(None)?;
    let manifests = build_profiles(&PlanRequest {
        root: store.root(),
        profile,
        platform,
        profiles: &settings.profiles,
        storage: identity,
    })?;
    store.save(&manifests, true)?;
    let manifest = manifests
        .get(profile)
        .ok_or_else(|| DomainError::new("selected plan was not built"))?;
    let alias = store.alias(profile, platform)?;
    let missing = store.missing(&manifests)?.len();
    Ok(PlanOutcome::Planned {
        profile: profile.into(),
        platform: platform.into(),
        digest: manifest.digest()?.as_str().to_owned(),
        alias: alias.display().to_string(),
        missing,
    })
}

/// Checks that the locked graph matches the reviewed inputs and is fully present.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when an identifier is invalid, no plan is locked,
/// a locked input changed, or an artifact is missing.
pub fn check_locked_profile<S: PlanStore>(
    store: &S,
    settings: &Settings,
    selection: &WorkspaceSelection,
) -> UseCaseResult<PlanOutcome> {
    let profile = selection.require_profile()?;
    let platform = selection.require_platform()?;
    identifier(profile)?;
    identifier(platform)?;
    let graph = locked_graph(store, settings, profile, platform)?;
    require_present(store, &graph, profile)?;
    Ok(PlanOutcome::Checked {
        profile: profile.into(),
        platform: platform.into(),
    })
}

/// Reads the legacy airlock plan and reports whether it is satisfied.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when the plan or CAS cannot be read.
pub fn check_airlock_plan<L: LegacyPlan>(plan: &L) -> UseCaseResult<PlanOutcome> {
    if !plan.exists() {
        return Err(DomainError::new(format!(
            "cannot evaluate lockfile drift: no airlock plan found at {}",
            plan.path()
        )));
    }
    match plan.drift()? {
        DriftState::InSync => Ok(PlanOutcome::AirlockInSync),
        DriftState::Drift {
            missing,
            mismatched_digests,
            commands,
        } => Ok(PlanOutcome::AirlockDrift {
            text: drift_report(&missing, &mismatched_digests, &commands),
        }),
    }
}

fn drift_report(missing: &[String], mismatched: &[String], commands: &[String]) -> String {
    use std::fmt::Write as _;
    let mut report = String::new();
    if !missing.is_empty() {
        report.push_str("Missing packages:\n");
        for label in missing {
            let _ = writeln!(report, "- {label}");
        }
    }
    if !mismatched.is_empty() {
        report.push_str("Mismatched digests:\n");
        for digest in mismatched {
            let _ = writeln!(report, "- {digest}");
        }
    }
    if !commands.is_empty() {
        report.push_str("Review and commit isolock.ingest.json with exact source, filename, and SHA-256 pins before dispatch. No registry resolution is performed.\nDispatch commands:\n");
        for command in commands {
            let _ = writeln!(report, "{command}");
        }
    }
    report.trim_end().to_owned()
}

/// Loads the locked graph and proves every constituent still matches configuration.
pub(crate) fn locked_graph<S: PlanStore>(
    store: &S,
    settings: &Settings,
    profile: &str,
    platform: &str,
) -> UseCaseResult<ProfileGraph> {
    let expected = store.load(profile, platform)?;
    let expected_graph = store.graph(expected.clone())?;
    preflight(settings, &expected_graph)?;
    let identity = store.storage_identity(Some(&expected.storage))?;
    let actual = build_profiles(&PlanRequest {
        root: store.root(),
        profile,
        platform,
        profiles: &settings.profiles,
        storage: identity,
    })?;
    let selected = actual
        .get(profile)
        .ok_or_else(|| DomainError::new("selected profile is missing"))?;
    let changed = isolock_plan::profile::changed_input(&expected, selected)?;
    if let Some(path) = changed {
        return Err(DomainError::new(format!(
            "Profile {profile}/{platform}: locked input changed: {path}"
        )));
    }
    store.graph(expected)
}

/// Fails when any reachable digest is absent, listing every missing artifact.
pub(crate) fn require_present<S: PlanStore>(
    store: &S,
    manifests: &ProfileGraph,
    profile: &str,
) -> UseCaseResult<()> {
    let missing = store.missing(manifests)?;
    if missing.is_empty() {
        return Ok(());
    }
    use std::fmt::Write as _;
    let mut report = format!("Profile {profile}: missing artifacts\n");
    for digest in missing {
        let labels = manifests
            .values()
            .flat_map(|manifest| manifest.artifacts.iter())
            .filter(|artifact| artifact.digest.as_str() == digest)
            .map(|artifact| {
                format!(
                    "{}=={} ({:?})",
                    artifact.name, artifact.version, artifact.kind
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        let _ = writeln!(
            report,
            "- {digest} {}",
            labels.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    Err(DomainError::new(report.trim_end().to_owned()))
}

/// Proves each locked constituent still matches its reviewed configuration and inputs.
pub(crate) fn preflight(settings: &Settings, manifests: &ProfileGraph) -> UseCaseResult<()> {
    for expected in manifests.values() {
        let mut actual = expected.clone();
        actual.specification = settings
            .profiles
            .get(&expected.profile)
            .ok_or_else(|| DomainError::new("locked profile was removed from configuration"))?
            .resolve(&expected.platform)?;
        if let Some(path) = isolock_plan::profile::changed_input(expected, &actual)? {
            return Err(DomainError::new(format!(
                "Profile {}/{}: locked input changed: {path}",
                expected.profile, expected.platform
            )));
        }
        for (path, digest) in &expected.inputs {
            let current = isolock_cas::digest_file(std::path::Path::new(path))
                .map_err(|_| DomainError::new(format!("locked input is missing: inputs.{path}")))?;
            if &current != digest {
                return Err(DomainError::new(format!(
                    "Profile {}/{}: locked input changed: inputs.{path}",
                    expected.profile, expected.platform
                )));
            }
        }
    }
    Ok(())
}

/// The manifest of one profile in a graph, with the reviewed missing-profile message.
pub(crate) fn selected<'a>(
    graph: &'a ProfileGraph,
    profile: &str,
) -> UseCaseResult<&'a ProfileManifest> {
    graph
        .get(profile)
        .ok_or_else(|| DomainError::new("selected profile is missing"))
}
