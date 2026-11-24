//! Immutable manifest objects, explicit GC roots, and serialized CAS mutations.

use crate::domain::Settings;
use crate::infrastructure::config;
use isolock_cas::{FileStore, Sha256Digest};
use isolock_plan::profile::{identifier, ProfileManifest};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Workspace {
    pub root: PathBuf,
    pub cas: FileStore,
}

impl Workspace {
    pub fn new(root: &Path, settings: &Settings) -> Self {
        Self {
            root: root.into(),
            cas: FileStore::new(config::absolute(root, &settings.storage.cas_dir)),
        }
    }
    pub fn plans(&self) -> PathBuf {
        self.root.join(".isolock/plans")
    }
    pub fn alias(&self, profile: &str, platform: &str) -> Result<PathBuf, String> {
        identifier(profile)?;
        identifier(platform)?;
        Ok(self.plans().join(format!("{profile}-{platform}.lock")))
    }
    pub fn object(&self, digest: &Sha256Digest) -> PathBuf {
        self.plans()
            .join("objects")
            .join(format!("{}.json", digest.as_str()))
    }
    pub fn load(&self, profile: &str, platform: &str) -> Result<ProfileManifest, String> {
        let bytes = fs::read(self.alias(profile, platform)?).map_err(|_| {
            "no locked profile plan; run isolock plan --profile NAME --platform PLATFORM first"
                .to_owned()
        })?;
        let manifest = decode(&bytes, None)?;
        if manifest.profile != profile || manifest.platform != platform {
            return Err("profile plan identity does not match its alias".into());
        }
        Ok(manifest)
    }
    pub fn load_object(&self, digest: &Sha256Digest) -> Result<ProfileManifest, String> {
        let bytes = fs::read(self.object(digest))
            .map_err(|_| format!("missing constituent manifest {}", digest.as_str()))?;
        decode(&bytes, Some(digest))
    }
    pub fn save(
        &self,
        manifests: &BTreeMap<String, ProfileManifest>,
        capture_inputs: bool,
    ) -> Result<(), String> {
        for manifest in manifests.values() {
            manifest.validate()?;
            if capture_inputs {
                for (path, digest) in &manifest.inputs {
                    self.cas
                        .put_file(digest, &self.root.join(path))
                        .map_err(|error| error.to_string())?;
                }
            }
            write_json(&self.object(&manifest.digest()?), manifest, true)?;
        }
        for manifest in manifests.values() {
            write_json(
                &self.alias(&manifest.profile, &manifest.platform)?,
                manifest,
                false,
            )?;
        }
        refresh_roots(self, &[])?;
        Ok(())
    }
    pub fn graph(
        &self,
        manifest: ProfileManifest,
    ) -> Result<BTreeMap<String, ProfileManifest>, String> {
        let mut graph = BTreeMap::new();
        self.visit(manifest, &mut graph, &mut BTreeSet::new())?;
        Ok(graph)
    }
    fn visit(
        &self,
        manifest: ProfileManifest,
        graph: &mut BTreeMap<String, ProfileManifest>,
        active: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        let digest = manifest.digest()?.as_str().to_owned();
        if !active.insert(digest.clone()) {
            return Err("manifest reference cycle".into());
        }
        for (name, child) in &manifest.constituents {
            let child = self.load_object(child)?;
            if child.profile != *name || child.platform != manifest.platform {
                return Err("constituent manifest identity mismatch".into());
            }
            self.visit(child, graph, active)?;
        }
        active.remove(&digest);
        if let Some(previous) = graph.insert(manifest.profile.clone(), manifest.clone()) {
            if previous != manifest {
                return Err("conflicting constituent profile manifests".into());
            }
        }
        Ok(())
    }
    pub fn missing(
        &self,
        manifests: &BTreeMap<String, ProfileManifest>,
    ) -> Result<Vec<String>, String> {
        let reachable = reachable(manifests);
        reachable
            .into_iter()
            .filter_map(|digest| match Sha256Digest::parse(&digest) {
                Err(error) => Some(Err(error.into())),
                Ok(sha) => match self.cas.verified(&sha) {
                    Ok(true) => None,
                    Ok(false) => Some(Ok(digest)),
                    Err(error) => Some(Err(error.to_string())),
                },
            })
            .collect()
    }
}

pub fn decode(bytes: &[u8], digest: Option<&Sha256Digest>) -> Result<ProfileManifest, String> {
    let manifest: ProfileManifest =
        serde_json::from_slice(bytes).map_err(|_| "invalid profile manifest schema".to_owned())?;
    manifest.validate()?;
    if let Some(expected) = digest {
        if &manifest.digest()? != expected {
            return Err("manifest object digest mismatch".into());
        }
    }
    Ok(manifest)
}

pub fn reachable(manifests: &BTreeMap<String, ProfileManifest>) -> BTreeSet<String> {
    manifests
        .values()
        .flat_map(ProfileManifest::reachable_blobs)
        .collect()
}

pub fn write_json(path: &Path, value: &impl Serialize, immutable: bool) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    write_file(path, &bytes, immutable)
}

pub fn write_file(path: &Path, bytes: &[u8], immutable: bool) -> Result<(), String> {
    let parent = path.parent().ok_or("output path has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    if immutable && path.exists() {
        let existing = fs::read(path).map_err(|error| error.to_string())?;
        if existing != bytes {
            return Err("refusing to mutate an immutable manifest or derived object".into());
        }
        return Ok(());
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temporary
        .write_all(bytes)
        .map_err(|error| error.to_string())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    if immutable {
        temporary
            .persist_noclobber(path)
            .map_err(|error| error.to_string())?;
    } else {
        temporary.persist(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Serializes sync, plan publication and GC across workspaces sharing a CAS.
pub struct StoreLease {
    path: PathBuf,
}
impl StoreLease {
    pub fn acquire(workspace: &Workspace) -> Result<Self, String> {
        fs::create_dir_all(workspace.cas.root()).map_err(|error| error.to_string())?;
        let path = workspace.cas.root().join(".isolock-operation-lock");
        fs::create_dir(&path).map_err(|_| "CAS is locked by another operation; remove .isolock-operation-lock only after confirming its owner has stopped".to_owned())?;
        Ok(Self { path })
    }
}
impl Drop for StoreLease {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GcReport {
    pub roots: Vec<String>,
    pub retained: Vec<String>,
    pub reclaimable: Vec<String>,
    pub bytes: u64,
    pub applied: bool,
}

/// A conservative, durable root registration shared by every workspace using this CAS.
/// Disappearing workspaces are not silently forgotten: stale registrations retain bytes.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RegisteredRoots {
    schema_version: u32,
    workspace: PathBuf,
    roots: Vec<String>,
    blobs: BTreeSet<String>,
}

fn root_selection(
    workspace: &Workspace,
    roots: &[String],
) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut selected = BTreeMap::new();
    if roots.is_empty() {
        if workspace.plans().exists() {
            for entry in fs::read_dir(workspace.plans()).map_err(|error| error.to_string())? {
                let path = entry.map_err(|error| error.to_string())?.path();
                if path
                    .extension()
                    .is_some_and(|extension| extension == "lock")
                {
                    selected.insert(
                        path.file_stem()
                            .ok_or("invalid plan alias")?
                            .to_string_lossy()
                            .into_owned(),
                        path,
                    );
                }
            }
        }
    } else {
        for root in roots {
            identifier(root)?;
            selected.insert(root.clone(), workspace.plans().join(format!("{root}.lock")));
        }
    }
    Ok(selected)
}

/// Called with the store lease held, after publishing a plan/receipt and before GC.
pub fn refresh_roots(
    workspace: &Workspace,
    roots: &[String],
) -> Result<(Vec<String>, BTreeSet<String>), String> {
    let selected = root_selection(workspace, roots)?;
    let (mut ingestion_roots, mut retained) =
        crate::infrastructure::ingestion::roots(&workspace.root)?;
    for path in selected.values() {
        let bytes =
            fs::read(path).map_err(|_| "GC root is missing; refusing collection".to_owned())?;
        retained.extend(reachable(&workspace.graph(decode(&bytes, None)?)?));
    }
    let receipts = workspace.root.join(".isolock/receipts");
    if receipts.exists() {
        for entry in fs::read_dir(receipts).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                let value: serde_json::Value =
                    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
                        .map_err(|_| "invalid receipt prevents safe GC")?;
                let digest = value
                    .get("manifest")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("receipt has no manifest root")?;
                let digest = Sha256Digest::parse(digest).map_err(str::to_owned)?;
                retained.extend(reachable(
                    &workspace.graph(workspace.load_object(&digest)?)?,
                ));
            }
        }
    }
    ingestion_roots.extend(selected.into_keys());
    ingestion_roots.sort();
    let roots = ingestion_roots;
    if !roots.is_empty() || !retained.is_empty() {
        let key = isolock_plan::profile::canonical_digest(&workspace.root)?;
        let registration = RegisteredRoots {
            schema_version: 1,
            workspace: workspace.root.clone(),
            roots: roots.clone(),
            blobs: retained.clone(),
        };
        write_json(
            &workspace
                .cas
                .root()
                .join(".isolock-roots")
                .join(format!("{}.json", key.as_str())),
            &registration,
            false,
        )?;
    }
    Ok((roots, retained))
}

pub fn garbage_collect(
    workspace: &Workspace,
    roots: &[String],
    apply: bool,
) -> Result<GcReport, String> {
    let _lease = StoreLease::acquire(workspace)?;
    let (roots, mut retained) = refresh_roots(workspace, roots)?;
    if roots.is_empty() && retained.is_empty() {
        return Err(
            "GC requires at least one explicit or discovered manifest or ingestion root".into(),
        );
    }
    // Other registered workspaces are independent roots, not collateral garbage.
    let registrations = workspace.cas.root().join(".isolock-roots");
    for entry in fs::read_dir(registrations).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("invalid shared CAS root registration".into());
        }
        let registration: RegisteredRoots =
            serde_json::from_slice(&fs::read(entry.path()).map_err(|error| error.to_string())?)
                .map_err(|_| "invalid shared CAS roots; refusing collection")?;
        if registration.schema_version != 1 {
            return Err("unsupported shared CAS root schema".into());
        }
        let expected = isolock_plan::profile::canonical_digest(&registration.workspace)?;
        if entry.file_name() != format!("{}.json", expected.as_str()).as_str() {
            return Err("shared CAS root identity mismatch".into());
        }
        for digest in &registration.blobs {
            Sha256Digest::parse(digest).map_err(str::to_owned)?;
        }
        retained.extend(registration.blobs);
    }
    let reclaimable = workspace
        .cas
        .digests()
        .map_err(|error| error.to_string())?
        .difference(&retained)
        .cloned()
        .collect::<Vec<_>>();
    let mut bytes = 0_u64;
    for digest in &reclaimable {
        let digest = Sha256Digest::parse(digest).map_err(str::to_owned)?;
        bytes = bytes
            .checked_add(
                fs::symlink_metadata(workspace.cas.path(&digest))
                    .map_err(|error| error.to_string())?
                    .len(),
            )
            .ok_or("GC byte count overflow")?;
    }
    if apply {
        for digest in &reclaimable {
            fs::remove_file(
                workspace
                    .cas
                    .path(&Sha256Digest::parse(digest).map_err(str::to_owned)?),
            )
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(GcReport {
        roots,
        retained: retained.into_iter().collect(),
        reclaimable,
        bytes,
        applied: apply,
    })
}
