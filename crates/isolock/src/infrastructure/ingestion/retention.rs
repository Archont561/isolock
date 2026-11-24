//! Build a fail-closed retained snapshot from reviewed batches and published profile roots.

use super::epoch::Policy;
use super::git::{generation, Publisher, MARKER_PATH};
use super::manifest::{Artifact, Manifest};
use super::publish::merge_artifact;
use crate::infrastructure::profile_store;
use isolock_cas::Sha256Digest;
use isolock_plan::profile::ProfileManifest;
use isolock_plan::AirlockPlan;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn closure(
    publisher: &Publisher,
    policy: &Policy,
    incoming: &Manifest,
) -> Result<(BTreeSet<String>, AirlockPlan), String> {
    let mut paths = BTreeSet::from([MARKER_PATH.into()]);
    let mut required = profile_roots(publisher, policy, &mut paths)?;
    let Reviews { batches, records } = Reviews::read(publisher)?;
    for digest in policy
        .retain_batches
        .iter()
        .chain(std::iter::once(&incoming.digest()?))
    {
        let batch = batches.get(digest.as_str()).ok_or(
            "reviewed retained batch is missing; publish its verified originals before migration",
        )?;
        paths.insert(format!("ingestion/objects/{}.json", digest.as_str()));
        required.extend(
            batch
                .artifacts
                .iter()
                .map(|artifact| artifact.sha256.as_str().to_owned()),
        );
    }
    let mut plan = AirlockPlan::default();
    for digest in required {
        let digest = Sha256Digest::parse(&digest).map_err(str::to_owned)?;
        publisher.verify_cas(&digest)?;
        paths.insert(format!("cas/{}", digest.sharded_path()));
        let mut found = false;
        for (path, artifact) in records
            .iter()
            .filter(|(_, artifact)| artifact.sha256 == digest)
        {
            let provenance: Vec<_> = batches
                .iter()
                .filter(|(_, batch)| batch.artifacts.contains(artifact))
                .map(|(digest, _)| digest)
                .collect();
            if provenance.is_empty() {
                return Err("retained original has no immutable reviewed batch provenance".into());
            }
            paths.extend(
                provenance
                    .into_iter()
                    .map(|digest| format!("ingestion/objects/{digest}.json")),
            );
            paths.insert(path.clone());
            merge_artifact(&mut plan, artifact)?;
            found = true;
        }
        if !found {
            return Err("retained original/input has no reviewed provenance record; re-ingest with a reviewed manifest before migration".into());
        }
    }
    Ok((paths, plan))
}

struct Reviews {
    batches: BTreeMap<String, Manifest>,
    records: BTreeMap<String, Artifact>,
}

impl Reviews {
    fn read(publisher: &Publisher) -> Result<Self, String> {
        let mut batches = BTreeMap::new();
        let mut records = BTreeMap::new();
        for path in publisher.entries.keys() {
            if path.starts_with("ingestion/objects/") {
                let batch: Manifest = serde_json::from_slice(&metadata(publisher, path)?)
                    .map_err(|_| "invalid retained review schema")?;
                batch.validate()?;
                if path != &format!("ingestion/objects/{}.json", batch.digest()?.as_str()) {
                    return Err("retained review digest mismatch".into());
                }
                batches.insert(batch.digest()?.as_str().to_owned(), batch);
            } else if path.starts_with("ingestion/records/") {
                let artifact: Artifact = serde_json::from_slice(&metadata(publisher, path)?)
                    .map_err(|_| "invalid retained provenance schema")?;
                Manifest {
                    schema_version: 1,
                    artifacts: vec![artifact.clone()],
                }
                .validate()?;
                if path != &format!("ingestion/records/{}.json", artifact.key()?.as_str()) {
                    return Err("retained provenance identity mismatch".into());
                }
                records.insert(path.clone(), artifact);
            }
        }
        Ok(Self { batches, records })
    }
}

fn metadata(publisher: &Publisher, path: &str) -> Result<Vec<u8>, String> {
    publisher
        .read(path)?
        .ok_or_else(|| "retained closure metadata is missing".into())
}

fn profile_roots(
    publisher: &Publisher,
    policy: &Policy,
    paths: &mut BTreeSet<String>,
) -> Result<BTreeSet<String>, String> {
    let mut queue = policy.retain_manifests.clone();
    for path in publisher.entries.keys().filter(|path| {
        path.starts_with("plans/")
            && std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "lock")
    }) {
        let bytes = metadata(publisher, path)?;
        let manifest = profile_store::decode(&bytes, None)?;
        if path != &format!("plans/{}-{}.lock", manifest.profile, manifest.platform) {
            return Err("retained profile alias identity mismatch".into());
        }
        let digest = manifest.digest()?;
        if metadata(
            publisher,
            &format!("plans/objects/{}.json", digest.as_str()),
        )? != bytes
        {
            return Err("retained profile alias differs from its immutable manifest".into());
        }
        queue.push(digest);
        paths.insert(path.clone());
    }
    let mut visited = BTreeSet::new();
    let mut required = BTreeSet::new();
    let mut pins = BTreeMap::new();
    while let Some(digest) = queue.pop() {
        if !visited.insert(digest.as_str().to_owned()) {
            continue;
        }
        let path = format!("plans/objects/{}.json", digest.as_str());
        let manifest = profile_store::decode(&metadata(publisher, &path)?, Some(&digest))?;
        verify_storage(publisher, &manifest, &mut pins)?;
        for (name, child) in &manifest.constituents {
            let bytes = metadata(publisher, &format!("plans/objects/{}.json", child.as_str()))?;
            let child_manifest = profile_store::decode(&bytes, Some(child))?;
            if child_manifest.profile != *name || child_manifest.platform != manifest.platform {
                return Err("retained constituent manifest identity mismatch".into());
            }
            queue.push(child.clone());
        }
        required.extend(manifest.reachable_blobs());
        paths.insert(path);
    }
    Ok(required)
}

fn verify_storage(
    publisher: &Publisher,
    manifest: &ProfileManifest,
    pins: &mut BTreeMap<(String, String), String>,
) -> Result<(), String> {
    let storage = &manifest.storage;
    if storage.kind != "git" {
        return Err("retained profiles must be locked to verified Git payload history; explicitly replan before migration".into());
    }
    let source = storage
        .repository
        .as_deref()
        .ok_or("retained profile has no storage source")?;
    let reference = storage
        .reference
        .as_deref()
        .ok_or("retained profile has no storage reference")?;
    let commit = storage
        .commit
        .as_deref()
        .ok_or("retained profile has no locked payload commit")?;
    generation(reference)?;
    // Every locked commit must remain reachable from its original, retained ref,
    // including locks carried through more than one orphan generation.
    let key = (source.to_owned(), reference.to_owned());
    let head = if let Some(head) = pins.get(&key) {
        head.clone()
    } else {
        publisher.pinned_source(source, commit)?;
        let head = publisher.fetch_ref(&format!("refs/heads/{reference}"))?.ok_or("retained profile payload reference is missing; restore the archived ref before migration")?;
        pins.insert(key, head.clone());
        head
    };
    if !publisher.is_ancestor(commit, &head)? {
        return Err("retained profile payload commit is not reachable from its locked reference; restore the archived history before migration".into());
    }
    for digest in manifest.reachable_blobs() {
        let digest = Sha256Digest::parse(&digest).map_err(str::to_owned)?;
        let path = format!("cas/{}", digest.sharded_path());
        let blob = publisher
            .entries
            .get(&path)
            .ok_or("retained profile is missing original/input bytes in the source snapshot")?;
        publisher.verify_object(commit, &path, &blob.object)?;
    }
    Ok(())
}
