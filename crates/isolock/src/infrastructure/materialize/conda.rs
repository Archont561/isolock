//! Derive a selected, local-file-only Conda workspace; seed Pixi alone links the prefix.
//!
//! Pixi's offline mode intentionally ignores remote mirrors on a cold cache. We therefore
//! derive (and retain) local lock/manifest inputs from the verified selected records. The
//! original project inputs and archives remain immutable and locked in the profile plan.

use super::{archive, payloads, seeds};
use crate::infrastructure::profile_store::{self, Workspace};
use isolock_plan::profile::{
    canonical_digest, identifier, Artifact, ArtifactKind, ProfileManifest,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn install(
    workspace: &Workspace,
    manifest: &ProfileManifest,
    pixi: &Path,
) -> Result<PathBuf, String> {
    let environment = manifest
        .specification
        .environment
        .as_deref()
        .ok_or("Pixi profile has no environment")?;
    let channel = channels(workspace, manifest)?;
    let project = workspace
        .root
        .join(".isolock/workspaces")
        .join(manifest.digest()?.as_str());
    derive_workspace(&project, &channel, manifest)?;
    let prefix = project.join(".pixi/envs").join(environment);
    let public_prefix = workspace
        .root
        .join(
            manifest
                .specification
                .pixi_manifest
                .as_deref()
                .ok_or("Pixi manifest input is missing")?,
        )
        .parent()
        .ok_or("Pixi manifest has no parent")?
        .join(".pixi/envs")
        .join(environment);
    if let Ok(metadata) = fs::symlink_metadata(&public_prefix) {
        if !metadata.file_type().is_symlink() {
            return Err("refusing to overwrite an unmanaged Pixi environment; move it aside before restoring the profile".into());
        }
    }
    run_pixi(workspace, manifest, pixi, &project, environment)?;
    verify_prefix(&prefix, manifest)?;
    seeds::link(&prefix, &public_prefix)?;
    Ok(public_prefix)
}

fn run_pixi(
    workspace: &Workspace,
    manifest: &ProfileManifest,
    pixi: &Path,
    project: &Path,
    environment: &str,
) -> Result<(), String> {
    // No user/project Pixi config or remote URLs are inherited by this derived workspace.
    let config = project.join("pixi-config.toml");
    profile_store::write_file(&config, b"offline = true\n", true)?;
    if project.join(".pixi/config.toml").exists() {
        return Err("unexpected configuration in an isolated Pixi workspace".into());
    }
    let status = Command::new(pixi)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", project.join("home"))
        .env("XDG_CACHE_HOME", project.join("cache"))
        .args([
            "install",
            "--environment",
            environment,
            "--locked",
            "--offline",
            "--manifest-path",
        ])
        .arg(project.join("pixi.toml"))
        .arg("--config-file")
        .arg(&config)
        .env("PIXI_CONFIG_FILE", &config)
        .env("PIXI_OFFLINE", "true")
        .env(
            "PIXI_CACHE_DIR",
            workspace
                .root
                .join(".isolock/cache/pixi")
                .join(manifest.digest()?.as_str()),
        )
        .env_remove("PIXI_CONDA_CHANNELS")
        .env_remove("PIXI_NO_CONFIG")
        .env_remove("PIXI_FROZEN")
        .status()
        .map_err(|_| "cannot execute the verified Pixi seed")?;
    if !status.success() {
        return Err("locked offline Pixi materialization failed".into());
    }
    Ok(())
}

fn channels(workspace: &Workspace, manifest: &ProfileManifest) -> Result<PathBuf, String> {
    let key = manifest.digest()?;
    let destination = workspace
        .root
        .join(".isolock/channels/conda")
        .join(key.as_str());
    if archive::reusable(&destination, key.as_str())? {
        return Ok(destination);
    }
    let stage = payloads::stage(&destination)?;
    let mut records: BTreeMap<(String, String), BTreeMap<String, serde_json::Value>> =
        BTreeMap::new();
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Conda)
    {
        add_package(workspace, artifact, stage.path(), &mut records)?;
    }
    for channel in records
        .keys()
        .map(|(channel, _)| channel.clone())
        .collect::<BTreeSet<_>>()
    {
        for subdir in [&manifest.platform, "noarch"] {
            records.entry((channel.clone(), subdir.into())).or_default();
        }
    }
    for ((channel, subdir), packages) in records {
        let (conda, legacy): (BTreeMap<_, _>, BTreeMap<_, _>) =
            packages.into_iter().partition(|(name, _)| {
                isolock_core::RegistryArchive::from_filename(name)
                    == Some(isolock_core::RegistryArchive::Conda)
            });
        let repodata = serde_json::json!({"repodata_version": 2, "info": {"subdir": subdir}, "packages": legacy, "packages.conda": conda, "removed": []});
        profile_store::write_json(
            &stage
                .path()
                .join(channel)
                .join(subdir)
                .join("repodata.json"),
            &repodata,
            true,
        )?;
    }
    archive::seal(stage.path(), key.as_str())?;
    archive::publish(stage, &destination)
}

fn record(artifact: &Artifact) -> &serde_json::Value {
    artifact
        .metadata
        .get("record")
        .unwrap_or(&artifact.metadata)
}

fn channel_key(artifact: &Artifact) -> Result<String, String> {
    let subdir = record(artifact)["subdir"]
        .as_str()
        .ok_or("Conda record has no subdir")?;
    identifier(subdir)?;
    let upstream = artifact
        .source
        .strip_suffix(&format!("/{subdir}/{}", artifact.filename))
        .ok_or("cannot determine locked Conda channel origin")?;
    Ok(canonical_digest(&upstream)?.as_str().to_owned())
}

fn add_package(
    workspace: &Workspace,
    artifact: &Artifact,
    stage: &Path,
    records: &mut BTreeMap<(String, String), BTreeMap<String, serde_json::Value>>,
) -> Result<(), String> {
    let record = record(artifact);
    let subdir = record["subdir"]
        .as_str()
        .ok_or("Conda record has no subdir")?;
    let channel = channel_key(artifact)?;
    if record["sha256"].as_str() != Some(artifact.digest.as_str()) {
        return Err("Conda record checksum differs from its locked artifact".into());
    }
    if !workspace
        .cas
        .verified(&artifact.digest)
        .map_err(|error| error.to_string())?
    {
        return Err("Conda package is missing from CAS".into());
    }
    let directory = stage.join(&channel).join(subdir);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(&artifact.filename);
    if path.exists() {
        return Err("Conda package filename collision".into());
    }
    fs::copy(workspace.cas.path(&artifact.digest), &path).map_err(|error| error.to_string())?;
    if isolock_cas::digest_file(&path).map_err(|error| error.to_string())? != artifact.digest {
        return Err("Conda archive changed while copying".into());
    }
    let size = fs::metadata(path).map_err(|error| error.to_string())?.len();
    if record["size"]
        .as_u64()
        .is_some_and(|expected| expected != size)
    {
        return Err("Conda archive size differs from its locked metadata".into());
    }
    let mut record = record.clone();
    record["size"] = size.into();
    records
        .entry((channel, subdir.into()))
        .or_default()
        .insert(artifact.filename.clone(), record);
    Ok(())
}

fn derive_workspace(
    project: &Path,
    channels: &Path,
    manifest: &ProfileManifest,
) -> Result<(), String> {
    let environment = manifest
        .specification
        .environment
        .as_deref()
        .ok_or("missing Pixi environment")?;
    let mut channel_urls = BTreeSet::new();
    let mut dependencies = toml::value::Table::new();
    let mut locations = Vec::new();
    let mut packages = Vec::new();
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Conda)
    {
        let record = record(artifact);
        let channel = file_url(&channels.join(channel_key(artifact)?))?;
        channel_urls.insert(channel.clone());
        let spec = toml::Value::Table(toml::value::Table::from_iter([
            ("version".into(), format!("=={}", artifact.version).into()),
            (
                "build".into(),
                record["build"]
                    .as_str()
                    .ok_or("Conda record has no build")?
                    .into(),
            ),
            ("channel".into(), channel.clone().into()),
        ]));
        if dependencies.insert(artifact.name.clone(), spec).is_some() {
            return Err("multiple Conda versions of one package in selected closure".into());
        }
        let location = format!(
            "{}{}/{}",
            channel,
            record["subdir"].as_str().ok_or("missing subdir")?,
            artifact.filename
        );
        locations.push(serde_json::json!({"conda": location}));
        let mut package = record.clone();
        package["conda"] = location.into();
        packages.push(package);
    }
    let toml = workspace_manifest(environment, &manifest.platform, &channel_urls, dependencies);
    // Pixi validates its implicit default environment too, even when installing a
    // named environment. It references the exact same closure and is never installed.
    let selected = serde_json::json!({"channels": channel_urls.into_iter().map(|url| serde_json::json!({"url":url})).collect::<Vec<_>>(), "packages": { &manifest.platform: locations }});
    let environments = BTreeMap::from([("default", &selected), (environment, &selected)]);
    let lock =
        serde_json::json!({"version": 6, "environments": environments, "packages": packages});
    profile_store::write_file(
        &project.join("pixi.toml"),
        toml::to_string(&toml)
            .map_err(|error| error.to_string())?
            .as_bytes(),
        true,
    )?;
    profile_store::write_json(&project.join("pixi.lock"), &lock, true)
}

fn workspace_manifest(
    environment: &str,
    platform: &str,
    channel_urls: &BTreeSet<String>,
    dependencies: toml::value::Table,
) -> toml::Value {
    toml::Value::Table(toml::value::Table::from_iter([
        (
            "workspace".into(),
            toml::Value::Table(toml::value::Table::from_iter([
                ("name".into(), "isolock-selected".into()),
                (
                    "channels".into(),
                    toml::Value::Array(
                        channel_urls.iter().map(|url| url.as_str().into()).collect(),
                    ),
                ),
                (
                    "platforms".into(),
                    toml::Value::Array(vec![platform.into()]),
                ),
            ])),
        ),
        ("dependencies".into(), toml::Value::Table(dependencies)),
        (
            "environments".into(),
            toml::Value::Table(toml::value::Table::from_iter([(
                environment.into(),
                toml::Value::Array(vec![]),
            )])),
        ),
    ]))
}

fn file_url(path: &Path) -> Result<String, String> {
    use std::fmt::Write;
    let path = path.to_str().ok_or("channel path is not UTF-8")?;
    if !path.starts_with('/') {
        return Err("local channel path must be absolute".into());
    }
    let mut url = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/._-~".contains(&byte) {
            url.push(char::from(byte));
        } else {
            write!(url, "%{byte:02X}").map_err(|error| error.to_string())?;
        }
    }
    url.push('/');
    Ok(url)
}

fn verify_prefix(prefix: &Path, manifest: &ProfileManifest) -> Result<(), String> {
    let expected = manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Conda)
        .map(|artifact| (artifact.name.clone(), artifact.digest.as_str().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let mut installed = BTreeMap::new();
    for entry in
        fs::read_dir(prefix.join("conda-meta")).map_err(|_| "Pixi did not create a Conda prefix")?
    {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
                .map_err(|_| "invalid installed Conda record")?;
        let name = record["name"]
            .as_str()
            .ok_or("installed Conda record has no name")?;
        let digest = record["sha256"]
            .as_str()
            .ok_or("installed Conda record has no SHA-256")?;
        if installed
            .insert(name.to_owned(), digest.to_owned())
            .is_some()
        {
            return Err("duplicate package in installed Conda prefix".into());
        }
    }
    if installed != expected {
        return Err(
            "installed Conda prefix does not match the selected locked artifact closure".into(),
        );
    }
    Ok(())
}
