//! Selective directory transport and an explicitly reported full-snapshot Git fallback.

use crate::application::ports::{BlobStore, ProcessRequest, ProcessRunner, UseCaseResult};
use crate::domain::{DomainError, Storage, Transport};
use crate::infrastructure::profile_store;
use isolock_cas::{FileStore, Sha256Digest};
use isolock_plan::profile::{canonical_digest, ProfileManifest, StorageIdentity};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum Snapshot {
    Directory(PathBuf),
    Git { repository: PathBuf, commit: String },
}

pub fn identity(
    processes: &dyn ProcessRunner,
    root: &Path,
    storage: &Storage,
    locked: Option<&StorageIdentity>,
) -> Result<StorageIdentity, String> {
    if storage.transport == Transport::Directory {
        if locked.is_some_and(|identity| identity.kind != "directory") {
            return Err("locked input changed: storage.kind".into());
        }
        return Ok(StorageIdentity {
            reference: Some(storage.reference.clone()),
            ..StorageIdentity::default()
        });
    }
    let source = storage
        .source
        .as_deref()
        .ok_or("Git transport requires storage.source")?;
    if let Some(locked) = locked {
        if locked.kind != "git"
            || locked.repository.as_deref() != Some(source)
            || locked.reference.as_deref() != Some(&storage.reference)
        {
            return Err("locked input changed: storage.repository or storage.reference".into());
        }
        return Ok(locked.clone());
    }
    let snapshot = git_snapshot(processes, root, source, &storage.reference)?;
    let Snapshot::Git { commit, .. } = snapshot else {
        return Err("Git snapshot was not created".into());
    };
    Ok(StorageIdentity {
        kind: "git".into(),
        repository: Some(source.into()),
        reference: Some(storage.reference.clone()),
        commit: Some(commit),
    })
}

pub fn snapshot(
    processes: &dyn ProcessRunner,
    root: &Path,
    storage: &Storage,
    locked: Option<&StorageIdentity>,
) -> Result<Snapshot, String> {
    let source = storage
        .source
        .as_deref()
        .ok_or("no artifact source configured; populate CAS or configure storage.source")?;
    match storage.transport {
        Transport::Directory => Ok(Snapshot::Directory(
            crate::infrastructure::config::absolute(root, Path::new(source)),
        )),
        Transport::Git => {
            let reference = locked
                .and_then(|identity| identity.commit.as_deref())
                .unwrap_or(&storage.reference);
            let snapshot = git_snapshot(processes, root, source, reference)?;
            if let (Some(expected), Snapshot::Git { commit, .. }) = (
                locked.and_then(|identity| identity.commit.as_ref()),
                &snapshot,
            ) {
                if expected != commit {
                    return Err("Git snapshot does not match locked storage commit".into());
                }
            }
            Ok(snapshot)
        }
    }
}

fn git_snapshot(
    processes: &dyn ProcessRunner,
    root: &Path,
    source: &str,
    reference: &str,
) -> Result<Snapshot, String> {
    if source.starts_with('-')
        || reference.starts_with('-')
        || reference.contains(['\0', '\n', '\r', ':'])
    {
        return Err("invalid Git source or reference".into());
    }
    let key = canonical_digest(&source)?;
    let repository = root.join(".isolock/transport").join(key.as_str());
    fs::create_dir_all(&repository).map_err(|error| error.to_string())?;
    git(processes, &repository, &["init", "--bare", "--quiet"])?;
    eprintln!("isolock transport: Git fallback fetches a broader storage snapshot; profile selection is local, not blob-selective transfer");
    git(
        processes,
        &repository,
        &[
            "fetch",
            "--quiet",
            "--depth=1",
            "--no-tags",
            "--",
            source,
            reference,
        ],
    )?;
    let commit = String::from_utf8(git(
        processes,
        &repository,
        &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
    )?)
    .map_err(|_| "Git commit is not UTF-8")?
    .trim()
    .to_owned();
    if !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid resolved Git commit".into());
    }
    Ok(Snapshot::Git { repository, commit })
}

fn git(
    processes: &dyn ProcessRunner,
    repository: &Path,
    arguments: &[&str],
) -> Result<Vec<u8>, String> {
    let output = processes
        .run(
            &ProcessRequest::new("git")
                .argument("-C")
                .argument(repository.to_string_lossy())
                .arguments(arguments.iter().copied()),
        )
        .map_err(|_| "cannot start Git transport")?;
    if !output.success {
        return Err(
            "Git transport failed; check the source, reference and repository access".into(),
        );
    }
    Ok(output.stdout)
}

impl Snapshot {
    pub fn manifest(
        &self,
        processes: &dyn ProcessRunner,
        digest: &Sha256Digest,
    ) -> Result<ProfileManifest, String> {
        let relative = format!("plans/objects/{}.json", digest.as_str());
        let bytes = match self {
            Self::Directory(root) => fs::read(root.join(relative)).map_err(|_| {
                "selected manifest is not available from the directory source".to_owned()
            })?,
            Self::Git { repository, commit } => git(
                processes,
                repository,
                &["show", &format!("{commit}:{relative}")],
            )?,
        };
        profile_store::decode(&bytes, Some(digest))
    }
    pub fn copy_blob(
        &self,
        processes: &dyn ProcessRunner,
        store: &mut dyn BlobStore,
        digest: &Sha256Digest,
    ) -> Result<bool, String> {
        if store.holds(digest).map_err(String::from)? {
            return Ok(false);
        }
        match self {
            Self::Directory(root) => {
                let source = FileStore::new(root.join("cas"));
                if !source.verified(digest).map_err(|error| error.to_string())? {
                    return Err(format!("source is missing artifact {}", digest.as_str()));
                }
                store
                    .put_file(digest, &source.path(digest))
                    .map(|()| true)
                    .map_err(String::from)
            }
            Self::Git { repository, commit } => {
                // The blob streams straight from Git into CAS, so a large artifact is
                // never buffered whole.
                let mut result = Ok(());
                let output = processes
                    .run_streaming(
                        &ProcessRequest::new("git")
                            .argument("-C")
                            .argument(repository.to_string_lossy())
                            .arguments([
                                "show".to_owned(),
                                format!("{commit}:cas/{}", digest.sharded_path()),
                            ]),
                        &mut |stream| {
                            let stream = stream.ok_or("Git blob stream is unavailable")?;
                            result = store.put_reader(digest, stream).map_err(String::from);
                            result.clone()
                        },
                    )
                    .map_err(|_| "cannot start Git blob retrieval")?;
                if !output.success {
                    return Err(format!(
                        "Git source is missing or cannot read artifact {}",
                        digest.as_str()
                    ));
                }
                result.map(|()| true)
            }
        }
    }
}

/// Fetch and verify the manifest graph before copying any payload bytes.
///
/// # Errors
/// Returns a message when a manifest is missing, mismatched, or the graph is inconsistent.
pub fn fetch_graph(
    processes: &dyn ProcessRunner,
    workspace: &mut dyn BlobStore,
    source: &Snapshot,
    digest: &Sha256Digest,
) -> Result<BTreeMap<String, ProfileManifest>, String> {
    let mut queue = vec![digest.clone()];
    let mut seen = BTreeSet::new();
    let mut manifests = BTreeMap::new();
    while let Some(digest) = queue.pop() {
        if !seen.insert(digest.as_str().to_owned()) {
            continue;
        }
        let manifest = source.manifest(processes, &digest)?;
        for child in manifest.constituents.values() {
            queue.push(child.clone());
        }
        if manifests
            .insert(manifest.profile.clone(), manifest)
            .is_some()
        {
            return Err("conflicting profile manifests in source graph".into());
        }
    }
    for manifest in manifests.values() {
        for (name, digest) in &manifest.constituents {
            let child = manifests
                .get(name)
                .ok_or("source is missing a constituent manifest")?;
            if child.digest()? != *digest || child.platform != manifest.platform {
                return Err("constituent manifest identity mismatch".into());
            }
        }
        workspace.write_manifest(manifest).map_err(String::from)?;
    }
    Ok(manifests)
}

/// Copies every reachable payload into CAS.
///
/// # Errors
/// Returns a message when a payload is missing from the source or cannot be stored.
pub fn fetch_blobs(
    processes: &dyn ProcessRunner,
    workspace: &mut dyn BlobStore,
    source: &Snapshot,
    manifests: &BTreeMap<String, ProfileManifest>,
) -> Result<usize, String> {
    let mut count = 0;
    for digest in profile_store::reachable(manifests) {
        let digest = Sha256Digest::parse(&digest).map_err(str::to_owned)?;
        if source.copy_blob(processes, workspace, &digest)? {
            count += 1;
        }
    }
    Ok(count)
}

/// Writes the selected graph as an offline directory snapshot.
///
/// # Errors
/// Returns a message when a blob is absent or a file cannot be written.
pub fn export(
    workspace: &mut dyn BlobStore,
    manifests: &BTreeMap<String, ProfileManifest>,
    output: &Path,
) -> Result<(), String> {
    let store = FileStore::new(output.join("cas"));
    for digest in profile_store::reachable(manifests) {
        let digest = Sha256Digest::parse(&digest).map_err(str::to_owned)?;
        if !workspace.holds(&digest).map_err(String::from)? {
            return Err("cannot export a profile with missing artifacts".into());
        }
        store
            .put_file(&digest, &workspace.path_of(&digest).map_err(String::from)?)
            .map_err(|error| error.to_string())?;
    }
    for manifest in manifests.values() {
        profile_store::write_json(
            &output.join(format!(
                "plans/objects/{}.json",
                manifest.digest()?.as_str()
            )),
            manifest,
            true,
        )?;
        profile_store::write_json(
            &output.join(format!(
                "plans/{}-{}.lock",
                manifest.profile, manifest.platform
            )),
            manifest,
            false,
        )?;
    }
    Ok(())
}

/// The transport port's fallible operations, expressed for `ArtifactTransport` adapters.
///
/// # Errors
/// Propagates the first transport failure as a [`DomainError`].
pub fn graph_for(
    processes: &dyn ProcessRunner,
    workspace: &mut dyn BlobStore,
    source: &Snapshot,
    digest: &Sha256Digest,
) -> UseCaseResult<BTreeMap<String, ProfileManifest>> {
    fetch_graph(processes, workspace, source, digest).map_err(DomainError::new)
}
