//! Immutable Cargo vendor, Python wheelhouse and JavaScript installation derivations.

use super::archive;
use crate::application::ports::ProcessRunner;
use crate::infrastructure::profile_store::{self, Workspace};
use isolock_cas::Sha256Digest;
use isolock_plan::profile::{
    canonical_digest, identifier, relative_path, verify_integrity, Artifact, ArtifactKind,
    ProfileManifest,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn cargo(
    processes: &dyn ProcessRunner,
    workspace: &Workspace,
    manifest: &ProfileManifest,
) -> Result<Option<PathBuf>, String> {
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
        cargo_package(processes, workspace, artifact, stage.path())?;
    }
    archive::seal(stage.path(), key)?;
    archive::publish(stage, &destination).map(Some)
}

fn cargo_package(
    processes: &dyn ProcessRunner,
    workspace: &Workspace,
    artifact: &Artifact,
    target: &Path,
) -> Result<(), String> {
    if artifact
        .metadata
        .get("origin")
        .and_then(|value| value.as_str())
        == Some("git")
    {
        return cargo_git_package(processes, workspace, artifact, target);
    }
    let package = format!("{}-{}", artifact.name, artifact.version);
    relative_path(&package)?;
    let unpacked = tempfile::tempdir().map_err(|error| error.to_string())?;
    archive::unpack(processes, &workspace.cas, artifact, unpacked.path())?;
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

/// Read the reviewed name and commit for one git artifact.
///
/// Planning records these, so a materialised tree can be described without
/// re-deriving anything from the lockfile.
fn git_identity(artifact: &Artifact) -> Result<(&str, &str), String> {
    let name = artifact
        .metadata
        .get("name")
        .and_then(|value| value.as_str())
        .ok_or("Cargo git artifact has no reviewed crate name")?;
    let commit = artifact
        .metadata
        .get("revision")
        .and_then(|value| value.as_str())
        .ok_or("Cargo git artifact has no reviewed commit")?;
    Ok((name, commit))
}

/// Materialises one Cargo git dependency as an exported commit tree.
///
/// A commit tree is not a crate: it has no enclosing `name-version` directory. It is
/// unpacked flat into a directory named after the commit, which Cargo accepts as a
/// vendored package directory.
///
/// Cargo *requires* a `.cargo-checksum.json` in every vendored directory and fails
/// outright without one, but it does not read or verify `package` for a git source,
/// and `package` is `null` for git packages in `cargo vendor` output. The `files` map
/// is written with real per-file digests because leaving it empty measurably removes
/// Cargo's own accidental-modification check. That check is not a security boundary
/// — the file's own comment says so, and it fails to read tampered bytes as a
/// vendored source rather than as the pinned commit. The anchor remains the CAS
/// digest verified before this runs.
fn cargo_git_package(
    processes: &dyn ProcessRunner,
    workspace: &Workspace,
    artifact: &Artifact,
    target: &Path,
) -> Result<(), String> {
    let (name, commit) = git_identity(artifact)?;
    let directory = git_directory(name, commit);
    relative_path(&directory)?;
    if target.join(&directory).exists() {
        return Err("Cargo git dependency directory collision".into());
    }
    let unpacked = tempfile::tempdir().map_err(|error| error.to_string())?;
    archive::unpack(processes, &workspace.cas, artifact, unpacked.path())?;
    if !unpacked.path().join("Cargo.toml").is_file() {
        return Err("Cargo git dependency archive must contain its crate root".into());
    }
    let files = archive::files(unpacked.path())?
        .into_iter()
        .map(|(name, (sha, _))| (name, sha))
        .collect::<BTreeMap<_, _>>();
    let checksum = unpacked.path().join(".cargo-checksum.json");
    if checksum.exists() {
        fs::remove_file(&checksum).map_err(|error| error.to_string())?;
    }
    profile_store::write_json(
        &checksum,
        &serde_json::json!({"files": files, "package": serde_json::Value::Null}),
        true,
    )?;
    archive::copy_tree(unpacked.path(), &target.join(directory))
}

/// The vendored directory name for one git dependency.
///
/// Cargo locates a vendored package as `<name>` or `<name>-<version>`. A git pin
/// adds the commit so two commits of one crate never share a directory, which is
/// what makes a stale tree detectable rather than silently reused.
fn git_directory(name: &str, commit: &str) -> String {
    format!("{name}-{}", &commit[..12.min(commit.len())])
}

pub fn wheels(
    _processes: &dyn ProcessRunner,
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
    processes: &dyn ProcessRunner,
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
        javascript_package(processes, workspace, artifact, stage.path())?;
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
    processes: &dyn ProcessRunner,
    workspace: &Workspace,
    artifact: &Artifact,
    target: &Path,
) -> Result<(), String> {
    let bytes =
        fs::read(workspace.cas.path(&artifact.digest)).map_err(|error| error.to_string())?;
    verify_integrity(artifact, &bytes)?;
    let unpacked = tempfile::tempdir().map_err(|error| error.to_string())?;
    archive::unpack(processes, &workspace.cas, artifact, unpacked.path())?;
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

/// The `[source]` key Cargo itself uses for a git dependency.
///
/// This must be byte-identical to the key in `Cargo.lock`'s `source` field, so it
/// is built from the same parts the lockfile records: the repository, the reviewed
/// reference, and the resolved commit.
fn git_source_key(artifact: &Artifact) -> Result<String, String> {
    let repository = artifact
        .metadata
        .get("repository")
        .and_then(|value| value.as_str())
        .ok_or("Cargo git artifact has no reviewed repository")?;
    let reference = artifact
        .metadata
        .get("reference")
        .and_then(|value| value.as_str())
        .ok_or("Cargo git artifact has no reviewed reference")?;
    let commit = artifact
        .metadata
        .get("revision")
        .and_then(|value| value.as_str())
        .ok_or("Cargo git artifact has no reviewed commit")?;
    Ok(format!(
        "git+{}?{reference}#{commit}",
        repository.trim_end_matches('/')
    ))
}

/// The `[source]` stanza that replaces one git dependency with our vendor tree.
///
/// A git URL is not a registry, so it must be written as `git`/`rev`. Emitting it
/// as `registry` — which is what the unscoped branch below would do — is rejected
/// by Cargo outright.
fn git_source(artifact: &Artifact) -> Result<toml::Value, String> {
    let repository = artifact
        .metadata
        .get("repository")
        .and_then(|value| value.as_str())
        .ok_or("Cargo git artifact has no reviewed repository")?;
    let reference = artifact
        .metadata
        .get("reference")
        .and_then(|value| value.as_str())
        .ok_or("Cargo git artifact has no reviewed reference")?;
    let commit = artifact
        .metadata
        .get("revision")
        .and_then(|value| value.as_str())
        .ok_or("Cargo git artifact has no reviewed commit")?;
    let (key, value) = reference
        .split_once('=')
        .ok_or("Cargo git artifact reference is malformed")?;
    if !matches!(key, "rev" | "tag" | "branch") || value != commit && key != "rev" {
        return Err("Cargo git artifact reference must pin its reviewed commit".into());
    }
    Ok(toml::Value::Table(toml::value::Table::from_iter([
        ("git".into(), repository.into()),
        (key.into(), value.into()),
        ("replace-with".into(), "isolock-vendor".into()),
    ])))
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
        if artifact
            .metadata
            .get("origin")
            .and_then(|value| value.as_str())
            == Some("git")
        {
            sources.insert(git_source_key(artifact)?, git_source(artifact)?);
            continue;
        }
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
