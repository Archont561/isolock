//! Local by default; connected acquisition is explicit and never resolves registries.

use super::git_dependency;
use super::manifest::Manifest;
use crate::application::ports::{ProcessRequest, ProcessRunner};
use isolock_cas::{digest_file, FileStore, SHARD_SIZE_BYTES};
use std::fs;
use std::path::Path;

/// The two temporary roots a reviewed batch stages into.
struct Staging<'a> {
    exports: &'a Path,
    store: &'a FileStore,
}

pub struct Staged {
    _directory: tempfile::TempDir,
    pub store: FileStore,
}

pub fn stage(
    processes: &dyn ProcessRunner,
    manifest: &Manifest,
    originals: &Path,
    download: bool,
) -> Result<Staged, String> {
    if !download
        && !fs::symlink_metadata(originals)
            .map_err(|_| "cannot read original artifact directory")?
            .file_type()
            .is_dir()
    {
        return Err("original artifacts must be in a regular directory".into());
    }
    let directory = tempfile::tempdir().map_err(|_| "cannot stage reviewed artifacts")?;
    let downloads = directory.path().join("downloads");
    fs::create_dir(&downloads).map_err(|_| "cannot stage reviewed downloads")?;
    let store = FileStore::new(directory.path().join("cas"));
    let exports = directory.path().join("exports");
    fs::create_dir(&exports).map_err(|_| "cannot stage reviewed commit exports")?;
    let staging = Staging {
        exports: &exports,
        store: &store,
    };
    for artifact in &manifest.artifacts {
        if let Some(pin) = &artifact.git {
            stage_git(processes, &staging, artifact, pin, download)?;
            continue;
        }
        let path = if download {
            download_one(processes, &downloads, artifact)?
        } else {
            originals.join(&artifact.filename)
        };
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "cannot read reviewed original artifact")?;
        if !metadata.file_type().is_file() {
            return Err("reviewed original artifact must be a regular file, not a symlink".into());
        }
        if metadata.len() > SHARD_SIZE_BYTES {
            return Err("reviewed ingestion currently supports originals up to 95 MiB; Git sharded publication is not implemented".into());
        }
        // Check each supplied original, even when two review records intentionally
        // share a digest. FileStore deduplication otherwise skips its reader.
        if digest_file(&path).map_err(|error| error.to_string())? != artifact.sha256 {
            return Err("reviewed original SHA-256 mismatch".into());
        }
        store
            .put_file(&artifact.sha256, &path)
            .map_err(|error| error.to_string())?;
    }
    Ok(Staged {
        _directory: directory,
        store,
    })
}

/// Exports one pinned commit and stores it, verifying every reviewed field.
///
/// A git dependency is acquired as an exported commit tree, never as a
/// downloaded file: the pin is the digest of `git archive` output, so a supplied
/// original cannot stand in for it.
fn stage_git(
    processes: &dyn ProcessRunner,
    staging: &Staging<'_>,
    artifact: &super::manifest::Artifact,
    pin: &super::manifest::GitPin,
    download: bool,
) -> Result<(), String> {
    if download {
        return Err("Cargo git dependencies are acquired by revision, not by URL".into());
    }
    let exported = git_dependency::export(
        processes,
        staging.exports,
        &artifact.name,
        &pin.repository,
        &pin.revision,
    )
    .map_err(String::from)?;
    // Every reviewed field is compared: a moved tag, an abbreviated revision, or
    // a rewrite between review and ingest fails here rather than in CAS.
    if exported.commit != pin.commit
        || exported.repository != pin.repository
        || exported.requested != pin.revision
        || exported.filename != artifact.filename
    {
        return Err("reviewed Cargo git revision resolved to a different commit".into());
    }
    git_dependency::store(staging.store, &artifact.sha256, &exported)
}

/// Acquires one reviewed HTTPS source into `downloads`.
///
/// Explicitly connected acquisition; the source was already validated as a
/// credential-free public HTTPS URL by the manifest.
fn download_one(
    processes: &dyn ProcessRunner,
    downloads: &Path,
    artifact: &super::manifest::Artifact,
) -> Result<std::path::PathBuf, String> {
    let path = downloads.join(&artifact.filename);
    let success = processes
        .run_discarding_output(
            &ProcessRequest::new("curl")
                .arguments([
                    "--disable",
                    "--globoff",
                    "--fail",
                    "--location",
                    "--silent",
                    "--show-error",
                    "--proto",
                    "=https",
                    "--proto-redir",
                    "=https",
                    "--max-time",
                    "300",
                    "--max-filesize",
                ])
                .argument(SHARD_SIZE_BYTES.to_string())
                .argument("--output")
                .argument(path.to_string_lossy())
                .argument("--")
                .argument(artifact.source.clone()),
        )
        .map_err(|_| "cannot start curl for explicitly connected ingestion")?;
    if !success.success {
        return Err("reviewed artifact download failed; no CAS batch was accepted".into());
    }
    Ok(path)
}
