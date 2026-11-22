//! Publish an existing verified profile export, pinned to an earlier payload commit.

use super::git::Publisher;
use crate::config::Overrides;
use crate::profile_cli::{self, Operation};
use crate::profile_store;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn publish_profile(
    root: &Path,
    overrides: &Overrides,
    reference: &str,
    remote: &str,
) -> Result<String, String> {
    let profile = overrides
        .profile
        .as_deref()
        .ok_or("profile publication requires --profile")?;
    let platform = overrides
        .platform
        .as_deref()
        .ok_or("profile publication requires --platform")?;
    let staging = tempfile::tempdir().map_err(|_| "cannot stage profile publication")?;
    let snapshot = staging.path().join("snapshot");
    // Reuse the locked configuration/input checks and CAS lease at the public
    // profile operation boundary; never synthesize or silently replan a lock here.
    profile_cli::run(
        overrides,
        Operation::Export {
            output: snapshot.clone(),
            quiet: true,
        },
    )?;
    let bytes = fs::read(snapshot.join(format!("plans/{profile}-{platform}.lock")))
        .map_err(|_| "cannot read exported profile")?;
    let manifest = profile_store::decode(&bytes, None)?;
    if manifest.storage.kind != "git" {
        return Err("profile publication requires a plan locked to Git payload storage".into());
    }
    let commit = manifest
        .storage
        .commit
        .as_deref()
        .ok_or("profile has no locked payload commit")?;
    let source = manifest
        .storage
        .repository
        .as_deref()
        .ok_or("profile has no locked storage source")?;
    let mut publisher = Publisher::open(root, remote, reference)?;
    publisher.pinned_source(source, commit)?;
    let files = files(&snapshot, &snapshot)?;
    // Every original, including reviewed profile input files, must already exist
    // in the pinned commit. Publishing a manifest cannot repair its locked history.
    for (relative, path) in &files {
        if relative.starts_with("cas/") {
            publisher.verify_pinned_blob(commit, relative, path)?;
        }
    }
    for (relative, path) in files {
        let alias = relative.starts_with("plans/")
            && Path::new(&relative)
                .extension()
                .is_some_and(|extension| extension == "lock");
        publisher.put_file(&relative, &path, alias)?;
    }
    publisher.finish(&format!("chore(deps): add profile/{profile}-{platform}"))
}

fn files(root: &Path, directory: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(|_| "cannot enumerate profile export")? {
        let entry = entry.map_err(|_| "cannot enumerate profile export")?;
        let kind = entry
            .file_type()
            .map_err(|_| "cannot inspect profile export")?;
        let path = entry.path();
        if kind.is_dir() {
            result.extend(files(root, &path)?);
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "invalid profile export path")?
                .to_str()
                .ok_or("profile export path is not UTF-8")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            result.insert(relative, path);
        } else {
            return Err("profile export contains an unsafe file type".into());
        }
    }
    Ok(result)
}
