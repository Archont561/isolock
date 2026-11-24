//! Reviewed budgets and an atomic, non-force boundary between retained storage epochs.

use super::git::{
    generation, object_id, text, Blob, Publisher, MARKER, MARKER_PATH, MAX_TREE_BYTES,
};
use super::manifest::{Manifest, MAX_MANIFEST_BYTES};
use super::publish::json;
use isolock_cas::Sha256Digest;
use isolock_plan::profile::canonical_digest;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const SEAL: &str = "Isolock-Epoch-Seal:";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct Policy {
    schema_version: u32,
    pub source_ref: String,
    pub source_commit: String,
    pub next_ref: String,
    max_live_bytes: u64,
    max_history_bytes: u64,
    pub retain_batches: Vec<Sha256Digest>,
    pub retain_manifests: Vec<Sha256Digest>,
}

impl Policy {
    pub fn read(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|_| "cannot read reviewed epoch policy")?
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read reviewed epoch policy")?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err("reviewed epoch policy exceeds 4 MiB".into());
        }
        let policy: Self =
            serde_json::from_slice(&bytes).map_err(|_| "invalid reviewed epoch policy schema")?;
        policy.validate()?;
        Ok(policy)
    }
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported epoch policy schema-version".into());
        }
        object_id(self.source_commit.as_bytes())?;
        let next = generation(&self.source_ref)?
            .checked_add(1)
            .ok_or("storage generation has no supported successor")?;
        if self.next_ref != format!("isolock/v{next}") {
            return Err("epoch policy must select the immediate next isolock/vN reference".into());
        }
        if self.max_live_bytes == 0
            || self.max_history_bytes == 0
            || self.max_live_bytes > MAX_TREE_BYTES
            || self.max_history_bytes > MAX_TREE_BYTES
        {
            return Err(
                "epoch policy live/history budgets must be positive and no larger than 1500 MiB"
                    .into(),
            );
        }
        for roots in [&self.retain_batches, &self.retain_manifests] {
            let unique: BTreeSet<_> = roots.iter().map(Sha256Digest::as_str).collect();
            if unique.len() != roots.len() {
                return Err("duplicate epoch retention root".into());
            }
        }
        Ok(())
    }
    fn fits(&self, live: u64, history: u64) -> bool {
        live <= self.max_live_bytes && history <= self.max_history_bytes
    }
    fn fits_orphan(&self, entries: &BTreeMap<String, Blob>) -> bool {
        let live = entries.values().map(|blob| blob.size).sum();
        let unique = entries
            .values()
            .map(|blob| (&blob.object, blob.size))
            .collect::<BTreeMap<_, _>>()
            .values()
            .sum();
        self.fits(live, unique)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Seal {
    schema_version: u32,
    policy: Sha256Digest,
    incoming_batch: Sha256Digest,
    next_ref: String,
    target_commit: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Record {
    schema_version: u32,
    policy: Policy,
    incoming_batch: Sha256Digest,
}

fn seal(publisher: &Publisher) -> Result<Option<Seal>, String> {
    let Some(parent) = &publisher.parent else {
        return Ok(None);
    };
    // An external append must not hide a seal and accidentally reopen an old epoch.
    let commits = publisher.run(
        &[
            "log",
            "--format=%H",
            "--fixed-strings",
            &format!("--grep={SEAL}"),
            parent,
        ],
        "epoch seal inspection",
    )?;
    let commits = text(&commits)?;
    if commits.is_empty() {
        return Ok(None);
    }
    if commits.lines().count() != 1 || commits != parent {
        return Err("storage epoch is sealed but its history was changed; inspect the successor and do not append".into());
    }
    let message = publisher.run(
        &["log", "-1", "--format=%B", parent],
        "epoch seal retrieval",
    )?;
    if message.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("invalid epoch seal metadata".into());
    }
    let records: Vec<_> = text(&message)?
        .lines()
        .filter_map(|line| line.strip_prefix(SEAL))
        .collect();
    if records.len() != 1 {
        return Err("invalid epoch seal metadata".into());
    }
    let seal: Seal = serde_json::from_str(records[0]).map_err(|_| "invalid epoch seal metadata")?;
    if seal.schema_version != 1 {
        return Err("unsupported epoch seal schema-version".into());
    }
    generation(&seal.next_ref)?;
    object_id(seal.target_commit.as_bytes())?;
    Ok(Some(seal))
}

fn validate_record(publisher: &Publisher) -> Result<(), String> {
    let Some(bytes) = publisher.read("epoch.json")? else {
        return Ok(());
    };
    let record: Record =
        serde_json::from_slice(&bytes).map_err(|_| "invalid epoch migration record")?;
    record.policy.validate()?;
    if record.schema_version != 1
        || publisher.reference != format!("refs/heads/{}", record.policy.next_ref)
    {
        return Err("invalid epoch migration record identity or schema-version".into());
    }
    let path = format!("ingestion/objects/{}.json", record.incoming_batch.as_str());
    let bytes = publisher
        .read(&path)?
        .ok_or("epoch migration record is missing its reviewed batch")?;
    let batch: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| "invalid epoch migration record batch")?;
    batch.validate()?;
    if batch.digest()? != record.incoming_batch {
        return Err("epoch migration record batch digest mismatch".into());
    }
    Ok(())
}

pub(super) fn review(
    publisher: &mut Publisher,
    policy: Option<&Policy>,
    incoming: Option<&Manifest>,
) -> Result<Option<String>, String> {
    validate_record(publisher)?;
    if let Some(seal) = seal(publisher)? {
        let Some(policy) = policy else {
            return Err(format!("storage epoch is sealed; explicitly review and adopt {} (configuration was not changed)", seal.next_ref));
        };
        let incoming = incoming.ok_or("epoch retry requires the accepted reviewed batch")?;
        return retry(publisher, policy, incoming, &seal).map(Some);
    }
    if let Some(policy) = policy {
        if publisher.reference != format!("refs/heads/{}", policy.source_ref)
            || publisher.parent.as_deref() != Some(&policy.source_commit)
        {
            return Err("epoch policy source-ref/source-commit does not match the current storage tip; inspect and review a new policy".into());
        }
    }
    Ok(None)
}

fn retry(
    publisher: &mut Publisher,
    policy: &Policy,
    incoming: &Manifest,
    seal: &Seal,
) -> Result<String, String> {
    if publisher.reference != format!("refs/heads/{}", policy.source_ref)
        || seal.policy != canonical_digest(policy)?
        || seal.incoming_batch != incoming.digest()?
        || seal.next_ref != policy.next_ref
    {
        return Err("storage epoch is sealed under a different reviewed policy or batch; explicitly review successor adoption".into());
    }
    let source = publisher
        .parent
        .as_deref()
        .ok_or("sealed source is missing")?;
    let parents = publisher.run(
        &["rev-list", "--parents", "-1", source],
        "accepted source seal ancestry",
    )?;
    if text(&parents)?.split_whitespace().collect::<Vec<_>>() != [source, &policy.source_commit] {
        return Err("accepted source seal parent does not match reviewed source-commit".into());
    }
    let trees = publisher.run(
        &[
            "rev-parse",
            &format!("{source}^{{tree}}"),
            &format!("{}^{{tree}}", policy.source_commit),
        ],
        "accepted source seal trees",
    )?;
    let trees: Vec<_> = text(&trees)?.lines().collect();
    if trees.len() != 2 || trees[0] != trees[1] {
        return Err("accepted source seal changed its snapshot bytes".into());
    }
    let head = publisher
        .fetch_ref(&format!("refs/heads/{}", seal.next_ref))?
        .ok_or("accepted epoch successor is missing; restore the archived ref before retry")?;
    if !publisher.is_ancestor(&seal.target_commit, &head)? {
        return Err("accepted epoch root is not reachable from its successor; inspect changed remote history".into());
    }
    let parents = publisher.run(
        &["rev-list", "--parents", "-1", &seal.target_commit],
        "accepted orphan root inspection",
    )?;
    if text(&parents)?.split_whitespace().count() != 1 {
        return Err("accepted epoch target is not an orphan root".into());
    }
    let record: Record =
        serde_json::from_slice(&publisher.read_at(&seal.target_commit, "epoch.json")?)
            .map_err(|_| "invalid accepted epoch migration record")?;
    if record.schema_version != 1
        || record.policy != *policy
        || record.incoming_batch != incoming.digest()?
    {
        return Err(
            "accepted epoch migration record does not match the reviewed publication".into(),
        );
    }
    verify_accepted_target(publisher, policy, incoming, &seal.target_commit)?;
    Ok(format!(
        "Storage is up to date: accepted rollover to {} at {}",
        seal.next_ref, seal.target_commit
    ))
}

fn verify_accepted_target(
    publisher: &mut Publisher,
    policy: &Policy,
    incoming: &Manifest,
    target: &str,
) -> Result<(), String> {
    let accepted = publisher.snapshot(target)?;
    if publisher.read_at(target, MARKER_PATH)? != MARKER {
        return Err("accepted epoch target has an invalid storage marker".into());
    }
    if !policy.fits_orphan(&accepted) {
        return Err("accepted epoch exceeds its reviewed budgets".into());
    }
    let batch_path = format!("ingestion/objects/{}.json", incoming.digest()?.as_str());
    if !accepted.contains_key(&batch_path) {
        return Err("accepted epoch is missing its reviewed batch".into());
    }
    let batch: Manifest = serde_json::from_slice(&publisher.read_at(target, &batch_path)?)
        .map_err(|_| "invalid accepted epoch reviewed batch")?;
    batch.validate()?;
    if batch.digest()? != incoming.digest()? {
        return Err("accepted epoch review digest mismatch".into());
    }
    let mut paths = BTreeSet::from([batch_path]);
    for artifact in &incoming.artifacts {
        paths.insert(format!("cas/{}", artifact.sha256.sharded_path()));
        paths.insert(format!(
            "ingestion/records/{}.json",
            artifact.key()?.as_str()
        ));
    }
    // Rebuild the originally required closure from the sealed SOURCE plus the
    // accepted incoming objects, not from whatever roots a damaged target kept.
    let mut candidate = publisher.entries.clone();
    for path in paths {
        let blob = accepted
            .get(&path)
            .ok_or("accepted epoch is missing reviewed original/provenance bytes")?;
        if candidate
            .get(&path)
            .is_some_and(|old| old.object != blob.object)
        {
            return Err("accepted epoch changed immutable source bytes".into());
        }
        candidate.insert(path, blob.clone());
    }
    let source = std::mem::replace(&mut publisher.entries, candidate);
    let result = super::retention::closure(publisher, policy, incoming);
    let candidate = std::mem::replace(&mut publisher.entries, source);
    let (paths, plan) = result?;
    for path in &paths {
        if accepted.get(path).map(|blob| &blob.object)
            != candidate.get(path).map(|blob| &blob.object)
        {
            return Err(
                "accepted epoch is missing or changed a required retained snapshot path".into(),
            );
        }
    }
    if accepted
        .keys()
        .any(|path| !paths.contains(path) && !matches!(path.as_str(), "plan.json" | "epoch.json"))
    {
        return Err("accepted epoch contains unreviewed snapshot paths".into());
    }
    let actual: isolock_plan::AirlockPlan =
        serde_json::from_slice(&publisher.read_at(target, "plan.json")?)
            .map_err(|_| "invalid accepted epoch airlock plan")?;
    if actual != plan {
        return Err("accepted epoch airlock plan does not match its reviewed closure".into());
    }
    Ok(())
}

pub(super) fn finish(
    mut publisher: Publisher,
    policy: Policy,
    incoming: &Manifest,
    message: &str,
) -> Result<String, String> {
    if policy.fits(publisher.live_bytes(), publisher.history_bytes()?) {
        return publisher.finish(message);
    }
    if publisher
        .fetch_ref(&format!("refs/heads/{}", policy.next_ref))?
        .is_some()
    {
        return Err("reviewed successor already exists; inspect it and review a new policy, never overwrite a generation".into());
    }
    let batch = incoming.digest()?;
    let (retained, plan) = super::retention::closure(&publisher, &policy, incoming)?;
    publisher.retain(&retained)?;
    publisher.put_bytes("plan.json", &json(&plan)?, true)?;
    publisher.put_bytes(
        "epoch.json",
        &json(&Record {
            schema_version: 1,
            policy: policy.clone(),
            incoming_batch: batch.clone(),
        })?,
        false,
    )?;
    if !policy.fits_orphan(&publisher.entries) {
        return Err("retained closure exceeds reviewed epoch budgets; increase the reviewed budgets or reduce explicit roots (published profiles are mandatory)".into());
    }
    let tree = publisher.tree()?;
    let target = publisher.commit(
        &tree,
        None,
        &format!("chore(deps): start {}", policy.next_ref),
    )?;
    let seal = serde_json::to_string(&Seal {
        schema_version: 1,
        policy: canonical_digest(&policy)?,
        incoming_batch: batch,
        next_ref: policy.next_ref.clone(),
        target_commit: target.clone(),
    })
    .map_err(|_| "cannot encode epoch seal")?;
    let old_tree = object_id(&publisher.run(
        &["rev-parse", &format!("{}^{{tree}}", policy.source_commit)],
        "source epoch tree lookup",
    )?)?;
    let seal = publisher.commit(
        &old_tree,
        Some(&policy.source_commit),
        &format!("chore(deps): seal {}\n\n{SEAL} {seal}", policy.source_ref),
    )?;
    publisher.atomic_epoch(&seal, &target, &policy.next_ref)?;
    Ok(format!("Published orphan {target} to {}; {} is sealed and retained. Explicitly review adoption; configuration was not changed.", policy.next_ref, policy.source_ref))
}
