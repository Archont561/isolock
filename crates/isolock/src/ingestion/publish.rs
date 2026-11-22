//! A reviewed publication merges at the current storage tip and never forces a ref.

use super::git::Publisher;
use super::manifest::{Artifact, Manifest};
use crate::config::{self, Overrides};
use crate::profile_store::{StoreLease, Workspace};
use isolock_plan::{AirlockPlan, PlannedArtifact};
use std::path::Path;

pub fn publish(
    overrides: &Overrides,
    path: &Path,
    remote: &str,
    selection: super::Selection<'_>,
) -> Result<String, String> {
    let root = std::env::current_dir().map_err(|_| "cannot resolve project directory")?;
    let settings = config::load(&root, overrides)?.settings;
    if overrides.profile.is_some() || overrides.platform.is_some() {
        if selection.registry.is_some() || !selection.packages.is_empty() {
            return Err("profile publication cannot select registry packages".into());
        }
        return super::profiles::publish_profile(
            &root,
            overrides,
            &settings.storage.reference,
            remote,
        );
    }
    let manifest =
        Manifest::read(&root.join(path))?.select(selection.registry, selection.packages)?;
    let workspace = Workspace::new(&root, &settings);
    let _lease = StoreLease::acquire(&workspace)?;
    for artifact in &manifest.artifacts {
        if !workspace
            .cas
            .verified(&artifact.sha256)
            .map_err(|error| error.to_string())?
        {
            return Err("reviewed artifact is missing from CAS; run isolock ingest first".into());
        }
    }
    let mut publisher = Publisher::open(&root, remote, &settings.storage.reference)?;
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
        &json(&manifest)?,
        false,
    )?;
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
    publisher.finish(&format!("chore(deps): add {identities}"))
}

fn json(value: &impl serde::Serialize) -> Result<Vec<u8>, String> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|_| "cannot serialize storage metadata")?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn merge_artifact(plan: &mut AirlockPlan, artifact: &Artifact) -> Result<(), String> {
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
