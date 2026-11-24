//! Immutable Cargo vendor, Python wheelhouse and JavaScript installation derivations.

use super::archive;
use crate::infrastructure::profile_store::{self, Workspace};
use isolock_cas::Sha256Digest;
use isolock_plan::profile::{
    canonical_digest, identifier, relative_path, verify_integrity, Artifact, ArtifactKind,
    ProfileManifest,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn cargo(workspace: &Workspace, manifest: &ProfileManifest) -> Result<Option<PathBuf>, String> {
    let Some((key, _)) = manifest.cargo_payloads.first_key_value() else {
        return Ok(None);
    };
    let destination = workspace.root.join(".isolock/vendor/cargo").join(key);
    if archive::reusable(&destination, key)? {
        return Ok(Some(destination));
    }
    let stage = stage(&destination)?;
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Cargo)
    {
        cargo_package(workspace, artifact, stage.path())?;
    }
    archive::seal(stage.path(), key)?;
    archive::publish(stage, &destination).map(Some)
}

fn cargo_package(workspace: &Workspace, artifact: &Artifact, target: &Path) -> Result<(), String> {
    let package = format!("{}-{}", artifact.name, artifact.version);
    relative_path(&package)?;
    let unpacked = tempfile::tempdir().map_err(|error| error.to_string())?;
    archive::unpack(&workspace.cas, artifact, unpacked.path())?;
    let source = unpacked.path().join(&package);
    if !source.is_dir()
        || fs::read_dir(unpacked.path())
            .map_err(|error| error.to_string())?
            .count()
            != 1
    {
        return Err("Cargo archive must contain exactly its name-version directory".into());
    }
    let checksum = source.join(".cargo-checksum.json");
    if checksum.exists() {
        fs::remove_file(&checksum).map_err(|error| error.to_string())?;
    }
    let files = archive::files(&source)?
        .into_iter()
        .map(|(name, (sha, _))| (name, sha))
        .collect::<BTreeMap<_, _>>();
    profile_store::write_json(
        &checksum,
        &serde_json::json!({"files": files, "package": artifact.digest}),
        true,
    )?;
    archive::copy_tree(&source, &target.join(package))
}

pub fn wheels(
    workspace: &Workspace,
    manifest: &ProfileManifest,
) -> Result<Option<PathBuf>, String> {
    let artifacts = manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Wheel)
        .collect::<Vec<_>>();
    if artifacts.is_empty()
        && manifest.specification.pypi_lock.is_none()
        && !manifest.specification.runtimes.contains_key("python")
    {
        return Ok(None);
    }
    let key = canonical_digest(&(1, &manifest.platform, &artifacts))?;
    let destination = workspace
        .root
        .join(".isolock/vendor/pypi")
        .join(key.as_str());
    if archive::reusable(&destination, key.as_str())? {
        return Ok(Some(destination));
    }
    let stage = stage(&destination)?;
    for artifact in artifacts {
        if !workspace
            .cas
            .verified(&artifact.digest)
            .map_err(|error| error.to_string())?
        {
            return Err("wheel is missing from CAS".into());
        }
        let path = stage.path().join(&artifact.filename);
        if path.exists() {
            return Err("wheel filename collision".into());
        }
        fs::copy(workspace.cas.path(&artifact.digest), &path).map_err(|error| error.to_string())?;
        if isolock_cas::digest_file(&path).map_err(|error| error.to_string())? != artifact.digest {
            return Err("wheel changed while copying".into());
        }
    }
    archive::seal(stage.path(), key.as_str())?;
    archive::publish(stage, &destination).map(Some)
}

pub fn javascript(
    workspace: &Workspace,
    manifest: &ProfileManifest,
) -> Result<Option<PathBuf>, String> {
    let artifacts = manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Javascript)
        .collect::<Vec<_>>();
    if artifacts.is_empty() && manifest.specification.javascript_lock.is_none() {
        return Ok(None);
    }
    let key = canonical_digest(&(1, &manifest.platform, &artifacts))?;
    let destination = workspace
        .root
        .join(".isolock/vendor/javascript")
        .join(key.as_str());
    if archive::reusable(&destination, key.as_str())? {
        return Ok(Some(destination));
    }
    let stage = stage(&destination)?;
    for artifact in &artifacts {
        javascript_package(workspace, artifact, stage.path())?;
    }
    for artifact in &artifacts {
        for installation in &artifact.destinations {
            javascript_bins(stage.path(), &destination, installation, &artifact.name)?;
        }
    }
    archive::seal(stage.path(), key.as_str())?;
    archive::publish(stage, &destination).map(Some)
}

fn javascript_package(
    workspace: &Workspace,
    artifact: &Artifact,
    target: &Path,
) -> Result<(), String> {
    let bytes =
        fs::read(workspace.cas.path(&artifact.digest)).map_err(|error| error.to_string())?;
    verify_integrity(artifact, &bytes)?;
    let unpacked = tempfile::tempdir().map_err(|error| error.to_string())?;
    archive::unpack(&workspace.cas, artifact, unpacked.path())?;
    let package = unpacked.path().join("package");
    if !package.is_dir()
        || fs::read_dir(unpacked.path())
            .map_err(|error| error.to_string())?
            .count()
            != 1
    {
        return Err("JavaScript tarballs must contain exactly a package directory".into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(
        &fs::read(package.join("package.json"))
            .map_err(|_| "JavaScript artifact has no package.json")?,
    )
    .map_err(|_| "invalid package.json in JavaScript artifact")?;
    if metadata["name"].as_str() != Some(&artifact.name)
        || metadata["version"].as_str() != Some(&artifact.version)
    {
        return Err("JavaScript artifact name/version disagrees with its lockfile".into());
    }
    for installation in &artifact.destinations {
        archive::copy_tree(&package, &target.join(installation))?;
    }
    Ok(())
}

fn javascript_bins(
    stage: &Path,
    final_root: &Path,
    installation: &str,
    package_name: &str,
) -> Result<(), String> {
    let package = stage.join(installation);
    let metadata: serde_json::Value = serde_json::from_slice(
        &fs::read(package.join("package.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|_| "invalid package.json")?;
    let mut bins = BTreeMap::new();
    if let Some(bin) = metadata["bin"].as_str() {
        bins.insert(
            package_name
                .rsplit('/')
                .next()
                .ok_or("package has no executable name")?
                .to_owned(),
            bin.to_owned(),
        );
    } else if let Some(bin) = metadata["bin"].as_object() {
        for (name, entry) in bin {
            bins.insert(
                name.clone(),
                entry
                    .as_str()
                    .ok_or("invalid package executable path")?
                    .to_owned(),
            );
        }
    }
    let prefix = installation
        .rsplit_once("node_modules/")
        .ok_or("invalid JavaScript installation")?
        .0;
    let bin_root = Path::new(prefix).join("node_modules/.bin");
    for (name, entry) in bins {
        identifier(&name)?;
        let entry = entry.strip_prefix("./").unwrap_or(&entry);
        relative_path(entry)?;
        archive::executable(&package.join(entry))?;
        let final_entry = final_root.join(installation).join(entry);
        let script = format!("#!/bin/sh\nexec {} \"$@\"\n", quote(&final_entry));
        let path = stage.join(&bin_root).join(name);
        profile_store::write_file(&path, script.as_bytes(), true)?;
        archive::executable(&path)?;
    }
    Ok(())
}

pub fn stage(destination: &Path) -> Result<tempfile::TempDir, String> {
    let parent = destination.parent().ok_or("destination has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    tempfile::Builder::new()
        .prefix(".isolock-stage-")
        .tempdir_in(parent)
        .map_err(|error| error.to_string())
}

pub fn quote(path: &Path) -> String {
    quote_string(&path.to_string_lossy())
}
pub fn quote_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn cargo_config(path: &Path, vendor: &Path, artifacts: &[Artifact]) -> Result<(), String> {
    let mut sources = toml::value::Table::new();
    sources.insert(
        "crates-io".into(),
        toml::Value::Table(toml::value::Table::from_iter([(
            "replace-with".into(),
            "isolock-vendor".into(),
        )])),
    );
    sources.insert(
        "isolock-vendor".into(),
        toml::Value::Table(toml::value::Table::from_iter([(
            "directory".into(),
            vendor.to_string_lossy().as_ref().into(),
        )])),
    );
    for artifact in artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Cargo)
    {
        let registry = artifact
            .source
            .strip_prefix("registry+")
            .unwrap_or(&artifact.source);
        if registry == "https://github.com/rust-lang/crates.io-index" {
            continue;
        }
        let key = Sha256Digest::from_bytes(registry.as_bytes());
        sources.insert(
            format!("isolock-{}", &key.as_str()[..12]),
            toml::Value::Table(toml::value::Table::from_iter([
                ("registry".into(), registry.into()),
                ("replace-with".into(), "isolock-vendor".into()),
            ])),
        );
    }
    let config = toml::Value::Table(toml::value::Table::from_iter([
        ("source".into(), toml::Value::Table(sources)),
        (
            "net".into(),
            toml::Value::Table(toml::value::Table::from_iter([(
                "offline".into(),
                toml::Value::Boolean(true),
            )])),
        ),
    ]));
    profile_store::write_file(
        &path.join("config.toml"),
        toml::to_string(&config)
            .map_err(|error| error.to_string())?
            .as_bytes(),
        true,
    )
}
