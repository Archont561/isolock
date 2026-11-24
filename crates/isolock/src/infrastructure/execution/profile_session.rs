//! Project one locked profile manifest into the plan+CAS shape the proxy serves.
//!
//! The proxy speaks one language: an [`AirlockPlan`] and an in-memory CAS. A profile
//! describes the same bytes with more structure, so this adapter is the only place that
//! knows both. It deliberately does not write `.isolock/plan.json`: a session must not
//! mutate state that planning and drift detection read.

use super::inputs;
use crate::application::ports::{ProfileSession, SessionSettings};
use isolock_core::Registry;
use isolock_plan::profile::{Artifact, ArtifactKind, ProfileManifest};
use isolock_plan::{AirlockPlan, PlannedArtifact, PlannedArtifactMetadata};
use std::path::PathBuf;

/// Projects the selected manifest into a session, or explains why it cannot.
///
/// # Errors
/// Returns a message when the materialized prefix is missing, so a profile session never
/// silently falls back to the host toolchain.
pub fn prepare(
    settings: &SessionSettings,
    manifest: &ProfileManifest,
    root: &std::path::Path,
) -> Result<SessionSettings, String> {
    let digest = manifest
        .digest()
        .map_err(|error| format!("cannot identify the selected profile: {error}"))?;
    let prefix = root
        .join(".isolock/profiles")
        .join(&manifest.profile)
        .join(&manifest.platform)
        .join(digest.as_str());
    if !prefix.join("bin").is_dir() {
        return Err(format!(
            "profile {}/{} is not materialized; run `isolock sync --profile {} --platform {}` first",
            manifest.profile, manifest.platform, manifest.profile, manifest.platform
        ));
    }
    let mut session = settings.clone();
    session.profile = Some(ProfileSession {
        profile: manifest.profile.clone(),
        platform: manifest.platform.clone(),
        cargo_home: prefix.join("bin/cargo-home"),
        prefix,
        plan: plan(manifest)?,
    });
    Ok(session)
}

/// Builds the registry-visible snapshot for the proxy.
///
/// Seeds and runtimes are toolchain prefixes, not packages a manager asks for, and a git
/// artifact is served from its local `[source]` stanza, so none of them belong here. The
/// match is exhaustive so a new kind must be classified deliberately.
///
/// # Errors
/// Returns a message when an artifact cannot be recorded.
fn plan(manifest: &ProfileManifest) -> Result<AirlockPlan, String> {
    let mut plan = AirlockPlan::new();
    for artifact in &manifest.artifacts {
        if let Some(registry) = registry(artifact.kind) {
            plan.add_with_metadata(
                PlannedArtifact {
                    registry,
                    name: artifact.name.clone(),
                    version: artifact.version.clone(),
                    digest: artifact.digest.clone(),
                },
                metadata(artifact),
            );
        }
    }
    Ok(plan)
}

/// The registry the proxy answers on, or `None` when the artifact is not a package.
fn registry(kind: ArtifactKind) -> Option<Registry> {
    match kind {
        ArtifactKind::Conda => Some(Registry::Conda),
        ArtifactKind::Cargo => Some(Registry::Cargo),
        ArtifactKind::Wheel => Some(Registry::Pypi),
        ArtifactKind::Javascript => Some(Registry::Npm),
        ArtifactKind::Seed | ArtifactKind::Runtime => None,
    }
}

/// Carries the filename and registry metadata so the proxy can answer a metadata request.
fn metadata(artifact: &Artifact) -> PlannedArtifactMetadata {
    PlannedArtifactMetadata {
        filename: Some(artifact.filename.clone()),
        subdir: None,
        registry_metadata: (!artifact.metadata.is_null()).then(|| artifact.metadata.clone()),
        requires_python: None,
    }
}

/// Loads the projected plan and its CAS blobs.
///
/// # Errors
/// Returns the same messages the legacy path emits for unreadable or invalid blobs.
pub fn load(settings: &SessionSettings) -> Result<(AirlockPlan, isolock_cas::InMemoryCas), String> {
    if let Some(profile) = &settings.profile {
        return Ok((profile.plan.clone(), inputs::cas(settings, &profile.plan)?));
    }
    inputs::load(settings)
}

/// The absolute `bin` directory a profile session puts first on `PATH`.
#[must_use]
pub fn prefix_bin(profile: &ProfileSession) -> PathBuf {
    profile.prefix.join("bin")
}
