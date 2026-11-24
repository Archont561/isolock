//! Versioned seed ownership; convenience links never select a locked provider.

use super::{archive, payloads};
use crate::infrastructure::profile_store::{self, Workspace};
use isolock_cas::digest_file;
use isolock_plan::profile::{ArchiveFormat, Artifact, ArtifactKind, ProfileManifest};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub fn provision(
    workspace: &Workspace,
    manifest: &ProfileManifest,
) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut tools = BTreeMap::new();
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Seed)
    {
        let destination = workspace
            .root
            .join(".isolock/bin/.seeds")
            .join(artifact.digest.as_str())
            .join(&artifact.name);
        let source = seed_source(workspace, artifact)?;
        if destination.exists() {
            if digest_file(&destination).map_err(|error| error.to_string())?
                != digest_file(&source.1).map_err(|error| error.to_string())?
            {
                return Err("a provisioned seed executable was modified".into());
            }
        } else {
            fs::create_dir_all(destination.parent().ok_or("seed has no parent")?)
                .map_err(|error| error.to_string())?;
            let temporary =
                tempfile::NamedTempFile::new_in(destination.parent().ok_or("seed has no parent")?)
                    .map_err(|error| error.to_string())?;
            fs::copy(&source.1, temporary.path()).map_err(|error| error.to_string())?;
            archive::executable(temporary.path())?;
            temporary
                .persist_noclobber(&destination)
                .map_err(|error| error.to_string())?;
        }
        archive::executable(&destination)?;
        link(
            &destination,
            &workspace.root.join(".isolock/bin").join(&artifact.name),
        )?;
        tools.insert(artifact.name.clone(), destination);
    }
    Ok(tools)
}

fn seed_source(
    workspace: &Workspace,
    artifact: &Artifact,
) -> Result<(tempfile::TempDir, PathBuf), String> {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    if !workspace
        .cas
        .verified(&artifact.digest)
        .map_err(|error| error.to_string())?
    {
        return Err("verified seed is missing from CAS; host fallback is forbidden".into());
    }
    let path = if archive::tool_format(artifact)? == ArchiveFormat::Raw {
        let path = directory.path().join("executable");
        fs::copy(workspace.cas.path(&artifact.digest), &path).map_err(|error| error.to_string())?;
        if digest_file(&path).map_err(|error| error.to_string())? != artifact.digest {
            return Err("seed changed while copying".into());
        }
        path
    } else {
        archive::unpack(&workspace.cas, artifact, directory.path())?;
        let entry = artifact
            .metadata
            .get("entry")
            .and_then(serde_json::Value::as_str)
            .ok_or("archived seed requires an entry")?;
        directory.path().join(entry)
    };
    archive::executable(&path)?;
    Ok((directory, path))
}

pub fn link(target: &Path, alias: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let parent = alias.parent().ok_or("link has no parent")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        if let Ok(metadata) = fs::symlink_metadata(alias) {
            if !metadata.file_type().is_symlink() {
                return Err("refusing to replace an unmanaged profile path".into());
            }
        }
        let temporary = tempfile::tempdir_in(parent).map_err(|error| error.to_string())?;
        let link = temporary.path().join("link");
        std::os::unix::fs::symlink(target, &link).map_err(|error| error.to_string())?;
        fs::rename(link, alias).map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (target, alias);
        Err("profile activation currently requires a POSIX host".into())
    }
}

pub fn launcher(
    path: &Path,
    executable: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut script = String::from("#!/bin/sh\n");
    for (key, value) in environment {
        let _ = writeln!(script, "export {key}={}", payloads::quote_string(value));
    }
    let _ = writeln!(script, "exec {} \"$@\"", payloads::quote(executable));
    profile_store::write_file(path, script.as_bytes(), true)?;
    archive::executable(path)
}

/// A simple reviewed shell-readable format, separate from environment payloads.
pub fn bootstrap_manifest(manifest: &ProfileManifest) -> Result<String, String> {
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    let file_digest = isolock_cas::Sha256Digest::from_bytes(&bytes);
    let mut result = format!(
        "manifest {} {} {} {} -\n",
        manifest.profile,
        manifest.digest()?.as_str(),
        manifest.platform,
        file_digest.as_str()
    );
    let mut chosen = BTreeMap::new();
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == ArtifactKind::Seed)
    {
        // A composed bootstrap only needs one Isolock executable to enter the typed flow.
        chosen.entry(artifact.name.clone()).or_insert(artifact);
    }
    for (name, artifact) in chosen {
        let format = match archive::tool_format(artifact)? {
            ArchiveFormat::Raw => "raw",
            ArchiveFormat::Tar => "tar",
            ArchiveFormat::TarGz => "tar-gz",
        };
        let entry = artifact
            .metadata
            .get("entry")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("-");
        if entry.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
        }) {
            return Err("seed entry cannot be represented in the POSIX seed manifest".into());
        }
        let _ = writeln!(
            result,
            "seed {name} {} {} {entry} {format}",
            artifact.digest.as_str(),
            artifact.filename
        );
    }
    Ok(result)
}
