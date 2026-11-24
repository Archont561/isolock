//! A reviewed publication merges at the current storage tip and never forces a ref.

use super::epoch::{self, Policy};
use super::git::Publisher;
use super::manifest::{Artifact, Manifest};
use crate::application::ports::{ProcessRunner, ProfileSnapshot, WorkspaceSelection};
use crate::domain::Settings;
use crate::infrastructure::profile_store::{StoreLease, Workspace};
use isolock_plan::{AirlockPlan, PlannedArtifact};
use std::path::Path;

/// One reviewed publication request.
pub struct Publication<'a> {
    /// The process boundary Git publication runs on.
    pub processes: &'a dyn ProcessRunner,
    /// Project root every relative path resolves against.
    pub root: &'a Path,
    /// Validated settings describing the storage reference.
    pub settings: &'a Settings,
    /// The explicit profile selection, when publishing a profile.
    pub workspace: &'a WorkspaceSelection,
    /// Exports the currently locked profile for publication.
    pub snapshot: &'a dyn ProfileSnapshot,
    /// Reviewed JSON pins already verified in CAS.
    pub path: &'a Path,
    /// Git remote name, local repository directory, or HTTPS URL.
    pub remote: &'a str,
    /// Reviewed epoch policy, when publication rolls over.
    pub epoch_policy: Option<&'a Path>,
    /// The reviewed ecosystem/package selection.
    pub selection: super::Selection<'a>,
}

pub fn publish(request: Publication<'_>) -> Result<String, String> {
    let Publication {
        processes,
        root,
        settings,
        workspace,
        snapshot,
        path,
        remote,
        epoch_policy,
        selection,
    } = request;
    // A profile publication names its target first, so the recovery message is publication-specific.
    if workspace.profile.is_some() || workspace.platform.is_some() {
        if epoch_policy.is_some() {
            return Err("epoch rollover requires a reviewed artifact batch, not profile publication; publish payloads and explicitly replan profile adoption separately".into());
        }
        if selection.registry.is_some() || !selection.packages.is_empty() {
            return Err("profile publication cannot select registry packages".into());
        }
        return super::profiles::publish_profile(
            processes,
            root,
            snapshot,
            &settings.storage.reference,
            remote,
        );
    }
    let manifest =
        Manifest::read(&root.join(path))?.select(selection.registry, selection.packages)?;
    let policy = epoch_policy
        .map(|path| Policy::read(&root.join(path)))
        .transpose()?;
    let mut publisher = Publisher::open(processes, root, remote, &settings.storage.reference)?;
    if let Some(status) = epoch::review(&mut publisher, policy.as_ref(), Some(&manifest))? {
        return Ok(status);
    }
    let workspace = Workspace::new(root, settings);
    let _lease = StoreLease::acquire(&workspace)?;
    stage_manifest(&mut publisher, &workspace, &manifest)?;
    let identities = manifest
        .artifacts
        .iter()
        .map(|artifact| {
            format!(
                "{}/{}=={}",
                artifact.registry, artifact.name, artifact.version
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let message = format!("chore(deps): add {identities}");
    match policy {
        Some(policy) => epoch::finish(publisher, policy, &manifest, &message),
        None => publisher.finish(&message),
    }
}

fn stage_manifest(
    publisher: &mut Publisher<'_>,
    workspace: &Workspace,
    manifest: &Manifest,
) -> Result<(), String> {
    for artifact in &manifest.artifacts {
        if !workspace
            .cas
            .verified(&artifact.sha256)
            .map_err(|error| error.to_string())?
        {
            return Err("reviewed artifact is missing from CAS; run isolock ingest first".into());
        }
    }
    let mut plan: AirlockPlan = publisher
        .read("plan.json")?
        .map(|bytes| {
            serde_json::from_slice(&bytes).map_err(|_| "published airlock plan is invalid")
        })
        .transpose()?
        .unwrap_or_default();
    for artifact in &manifest.artifacts {
        publisher.put_file(
            &format!("cas/{}", artifact.sha256.sharded_path()),
            &workspace.cas.path(&artifact.sha256),
            false,
        )?;
        publisher.put_bytes(
            &format!("ingestion/records/{}.json", artifact.key()?.as_str()),
            &json(artifact)?,
            false,
        )?;
        merge_artifact(&mut plan, artifact)?;
    }
    publisher.put_bytes("plan.json", &json(&plan)?, true)?;
    publisher.put_bytes(
        &format!("ingestion/objects/{}.json", manifest.digest()?.as_str()),
        &json(manifest)?,
        false,
    )?;
    Ok(())
}

pub(super) fn json(value: &impl serde::Serialize) -> Result<Vec<u8>, String> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|_| "cannot serialize storage metadata")?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn merge_artifact(plan: &mut AirlockPlan, artifact: &Artifact) -> Result<(), String> {
    if let Some(registry) = artifact.registry() {
        let mut metadata = artifact.metadata.clone();
        metadata.filename = Some(artifact.filename.clone());
        let previous = plan
            .artifacts_with_metadata()
            .find(|(previous, _)| {
                previous.registry == registry
                    && previous.name == artifact.name
                    && previous.version == artifact.version
            })
            .map(|(previous, metadata)| (previous.clone(), metadata.clone()));
        if let Some((previous, old_metadata)) = previous {
            if previous.digest != artifact.sha256 || old_metadata != metadata {
                return Err(
                    "published package identity already has different bytes or metadata".into(),
                );
            }
        } else {
            plan.add_with_metadata(
                PlannedArtifact {
                    registry,
                    name: artifact.name.clone(),
                    version: artifact.version.clone(),
                    digest: artifact.sha256.clone(),
                },
                metadata,
            );
        }
    }
    Ok(())
}
