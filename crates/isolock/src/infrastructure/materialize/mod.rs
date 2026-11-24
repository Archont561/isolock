//! Profile-local launchers over verified seeds, runtime prefixes and shared payloads.

mod archive;
mod conda;
mod launchers;
mod payloads;
mod seeds;

pub use seeds::bootstrap_manifest;

use crate::infrastructure::profile_store::{self, Workspace};
use isolock_cas::{digest_file, Sha256Digest};
use isolock_plan::profile::{
    canonical_digest, ArchiveFormat, Artifact, ArtifactKind, ProfileManifest, Provider,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct Receipt<'a> {
    schema_version: u32,
    manifest: Sha256Digest,
    profile: &'a str,
    platform: &'a str,
    provider: Provider,
    environment: &'a Option<String>,
    prefix: PathBuf,
    artifacts: &'a [Artifact],
    materializers: &'a BTreeMap<String, u32>,
    linked_artifacts: BTreeMap<String, Vec<Sha256Digest>>,
    payloads: BTreeMap<String, PathBuf>,
    derived_inputs: BTreeMap<PathBuf, Sha256Digest>,
}

#[derive(Default)]
struct Derived {
    seeds: BTreeMap<String, PathBuf>,
    runtimes: BTreeMap<String, PathBuf>,
    cargo: Option<PathBuf>,
    wheels: Option<PathBuf>,
    javascript: Option<PathBuf>,
    pixi_prefix: Option<PathBuf>,
}

impl Derived {
    fn provision(workspace: &Workspace, manifest: &ProfileManifest) -> Result<Self, String> {
        if manifest.specification.provider == Provider::Composed {
            return Ok(Self::default());
        }
        let seeds = seeds::provision(workspace, manifest)?;
        let pixi_prefix = if manifest.specification.provider == Provider::Pixi {
            Some(conda::install(
                workspace,
                manifest,
                seeds.get("pixi").ok_or("locked Pixi seed is missing")?,
            )?)
        } else {
            None
        };
        let mut runtimes = BTreeMap::new();
        for artifact in manifest
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::Runtime)
        {
            runtimes.insert(artifact.name.clone(), runtime(workspace, artifact)?);
        }
        Ok(Self {
            seeds,
            runtimes,
            pixi_prefix,
            cargo: payloads::cargo(workspace, manifest)?,
            wheels: payloads::wheels(workspace, manifest)?,
            javascript: payloads::javascript(workspace, manifest)?,
        })
    }

    fn paths(&self) -> BTreeMap<String, PathBuf> {
        let mut paths = BTreeMap::new();
        for (kind, path) in [
            ("cargo", &self.cargo),
            ("pypi", &self.wheels),
            ("javascript", &self.javascript),
            ("conda-prefix", &self.pixi_prefix),
        ] {
            if let Some(path) = path {
                paths.insert(kind.into(), path.clone());
            }
        }
        for (name, path) in &self.runtimes {
            paths.insert(format!("runtime-{name}"), path.clone());
        }
        paths
    }
}

pub fn install(
    workspace: &Workspace,
    manifests: &BTreeMap<String, ProfileManifest>,
    selected: &str,
) -> Result<PathBuf, String> {
    let manifest = manifests
        .get(selected)
        .ok_or("selected manifest is missing")?;
    let mut prefixes = BTreeMap::new();
    install_one(workspace, manifests, manifest, &mut prefixes)?;
    prefixes
        .remove(selected)
        .ok_or_else(|| "selected profile was not materialized".into())
}

fn install_one(
    workspace: &Workspace,
    manifests: &BTreeMap<String, ProfileManifest>,
    manifest: &ProfileManifest,
    prefixes: &mut BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    if prefixes.contains_key(&manifest.profile) {
        return Ok(());
    }
    for name in manifest.constituents.keys() {
        install_one(
            workspace,
            manifests,
            manifests.get(name).ok_or("constituent is missing")?,
            prefixes,
        )?;
    }
    let digest = manifest.digest()?;
    let destination = workspace
        .root
        .join(".isolock/profiles")
        .join(&manifest.profile)
        .join(&manifest.platform)
        .join(digest.as_str());
    let derived = Derived::provision(workspace, manifest)?;
    if !archive::reusable(&destination, digest.as_str())? {
        let stage = payloads::stage(&destination)?;
        launchers::create(
            workspace,
            manifest,
            &derived,
            launchers::Paths {
                stage: stage.path(),
                destination: &destination,
            },
            prefixes,
        )?;
        archive::seal(stage.path(), digest.as_str())?;
        archive::publish(stage, &destination)?;
    }
    if let Some(javascript) = &derived.javascript {
        let lock = manifest
            .specification
            .javascript_lock
            .as_deref()
            .map(Path::new)
            .and_then(Path::parent)
            .unwrap_or(Path::new(""));
        let alias = workspace.root.join(lock).join("node_modules");
        let target = javascript.join("node_modules");
        if fs::read_link(&alias).is_ok_and(|current| current != target) {
            return Err("another JavaScript profile owns node_modules; use a separate workspace or explicitly remove the old projection".into());
        }
        seeds::link(&target, &alias)?;
    }
    publish_receipt(workspace, manifest, &destination, &derived)?;
    seeds::link(
        &destination.join("bin"),
        &destination
            .parent()
            .ok_or("profile has no parent")?
            .join("bin"),
    )?;
    profile_store::write_file(
        &workspace
            .root
            .join(".isolock/activate")
            .join(format!("{}-{}", manifest.profile, manifest.platform)),
        format!(". {}\n", payloads::quote(&destination.join("activate"))).as_bytes(),
        false,
    )?;
    profile_store::refresh_roots(workspace, &[])?;
    prefixes.insert(manifest.profile.clone(), destination);
    Ok(())
}

fn publish_receipt(
    workspace: &Workspace,
    manifest: &ProfileManifest,
    destination: &Path,
    derived: &Derived,
) -> Result<(), String> {
    let mut inputs = BTreeMap::new();
    let digest = manifest.digest()?;
    if derived.pixi_prefix.is_some() {
        let project = workspace
            .root
            .join(".isolock/workspaces")
            .join(digest.as_str());
        for name in ["pixi.toml", "pixi.lock", "pixi-config.toml"] {
            let path = project.join(name);
            inputs.insert(
                path.clone(),
                digest_file(&path).map_err(|error| error.to_string())?,
            );
        }
        let seal = workspace
            .root
            .join(".isolock/channels/conda")
            .join(digest.as_str())
            .join(".isolock-tree.json");
        inputs.insert(
            seal.clone(),
            digest_file(&seal).map_err(|error| error.to_string())?,
        );
    }
    let mut paths = derived.paths();
    for child in manifest.constituents.keys() {
        paths.insert(
            format!("constituent-{child}"),
            workspace
                .root
                .join(".isolock/receipts")
                .join(format!("{child}-{}.json", manifest.platform)),
        );
    }
    let receipt = Receipt {
        schema_version: 1,
        manifest: digest,
        profile: &manifest.profile,
        platform: &manifest.platform,
        provider: manifest.specification.provider,
        environment: &manifest.specification.environment,
        prefix: derived
            .pixi_prefix
            .clone()
            .unwrap_or_else(|| destination.into()),
        artifacts: &manifest.artifacts,
        materializers: &manifest.materializers,
        linked_artifacts: linked_artifacts(&manifest.artifacts),
        payloads: paths,
        derived_inputs: inputs,
    };
    profile_store::write_json(
        &workspace
            .root
            .join(".isolock/receipts")
            .join(format!("{}-{}.json", manifest.profile, manifest.platform)),
        &receipt,
        false,
    )
}

fn linked_artifacts(artifacts: &[Artifact]) -> BTreeMap<String, Vec<Sha256Digest>> {
    let mut linked_artifacts: BTreeMap<String, Vec<Sha256Digest>> = BTreeMap::new();
    for artifact in artifacts {
        let kind = match artifact.kind {
            ArtifactKind::Conda => "conda-prefix",
            ArtifactKind::Cargo => "cargo-vendor",
            ArtifactKind::Wheel => "wheelhouse",
            ArtifactKind::Javascript => "node-modules",
            ArtifactKind::Seed => "seeds",
            ArtifactKind::Runtime => "runtimes",
        };
        let digests = linked_artifacts.entry(kind.into()).or_default();
        if !digests.contains(&artifact.digest) {
            digests.push(artifact.digest.clone());
        }
    }
    linked_artifacts
}

fn runtime(workspace: &Workspace, artifact: &Artifact) -> Result<PathBuf, String> {
    let key = canonical_digest(&(1, artifact))?;
    let destination = workspace.root.join(".isolock/runtimes").join(key.as_str());
    let entry = artifact
        .metadata
        .get("entry")
        .and_then(serde_json::Value::as_str);
    if !archive::reusable(&destination, key.as_str())? {
        let stage = payloads::stage(&destination)?;
        if archive::tool_format(artifact)? == ArchiveFormat::Raw {
            if !workspace
                .cas
                .verified(&artifact.digest)
                .map_err(|error| error.to_string())?
            {
                return Err("runtime is missing from CAS".into());
            }
            fs::copy(
                workspace.cas.path(&artifact.digest),
                stage.path().join(&artifact.name),
            )
            .map_err(|error| error.to_string())?;
            if digest_file(&stage.path().join(&artifact.name)).map_err(|error| error.to_string())?
                != artifact.digest
            {
                return Err("runtime changed while copying".into());
            }
            archive::executable(&stage.path().join(&artifact.name))?;
        } else {
            archive::unpack(&workspace.cas, artifact, stage.path())?;
        }
        archive::seal(stage.path(), key.as_str())?;
        archive::publish(stage, &destination)?;
    }
    Ok(if archive::tool_format(artifact)? == ArchiveFormat::Raw {
        destination.join(&artifact.name)
    } else {
        entry.map_or(destination.clone(), |entry| destination.join(entry))
    })
}

fn executable(prefix: &Path, name: &str) -> Result<PathBuf, String> {
    let path = prefix.join("bin").join(name);
    if !path.is_file() {
        return Err(format!(
            "verified runtime prefix does not provide bin/{name}; host fallback is forbidden"
        ));
    }
    Ok(path)
}
