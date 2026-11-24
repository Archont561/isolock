//! Verified archive extraction using the same native tar boundary as POSIX bootstrap.
//! Version one accepts link-free relocatable prefixes and rejects traversal/special files.

use crate::application::ports::{ProcessRequest, ProcessRunner};
use crate::infrastructure::profile_store;
use isolock_cas::{digest_file, FileStore, Sha256Digest};
use isolock_plan::profile::{relative_path, ArchiveFormat, Artifact};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn unpack(
    processes: &dyn ProcessRunner,
    cas: &FileStore,
    artifact: &Artifact,
    target: &Path,
) -> Result<(), String> {
    if !cas
        .verified(&artifact.digest)
        .map_err(|error| error.to_string())?
    {
        return Err(format!("missing artifact {}", artifact.digest.as_str()));
    }
    // The utility only reads our private, verified copy, not a mutable external path.
    let private = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    fs::copy(cas.path(&artifact.digest), private.path()).map_err(|error| error.to_string())?;
    if digest_file(private.path()).map_err(|error| error.to_string())? != artifact.digest {
        return Err("archive changed while copying".into());
    }
    inspect(processes, private.path())?;
    fs::create_dir_all(target).map_err(|error| error.to_string())?;
    let extracted = processes
        .run(
            &ProcessRequest::new("tar")
                .without_variables(["TAR_OPTIONS", "GZIP"])
                .arguments(["-xf"])
                .argument(private.path().to_string_lossy())
                .argument("-C")
                .argument(target.to_string_lossy())
                .argument("--no-same-owner"),
        )
        .map_err(|_| "cannot start tar")?;
    if !extracted.success {
        return Err("cannot extract verified archive".into());
    }
    // Defense in depth: do not publish a tree containing links or special filesystem nodes.
    files(target)?;
    normalize_permissions(target)?;
    Ok(())
}

fn inspect(processes: &dyn ProcessRunner, path: &Path) -> Result<(), String> {
    for verbose in [false, true] {
        let output = processes
            .run(
                &ProcessRequest::new("tar")
                    .without_variables(["TAR_OPTIONS", "GZIP"])
                    .argument(if verbose { "-tvf" } else { "-tf" })
                    .argument(path.to_string_lossy()),
            )
            .map_err(|_| "cannot inspect archive")?;
        if !output.success {
            return Err("invalid archive".into());
        }
        let listing =
            String::from_utf8(output.stdout).map_err(|_| "archive paths must be UTF-8")?;
        for line in listing.lines() {
            if verbose {
                if !matches!(line.as_bytes().first(), Some(b'-' | b'd')) {
                    return Err("archives must contain only regular files and directories; links and special files are rejected".into());
                }
            } else {
                let path = line.trim_end_matches('/').trim_start_matches("./");
                if path.is_empty() || path == "." {
                    continue;
                }
                relative_path(path)?;
                if path == ".isolock-tree.json" {
                    return Err("archive uses a reserved derivation receipt path".into());
                }
                if path.chars().any(char::is_control) {
                    return Err("archive paths cannot contain control characters".into());
                }
            }
        }
    }
    Ok(())
}

pub fn tool_format(artifact: &Artifact) -> Result<ArchiveFormat, String> {
    let value = artifact
        .metadata
        .get("format")
        .ok_or("tool artifact has no format")?;
    serde_json::from_value(value.clone()).map_err(|_| "invalid tool archive format".into())
}

pub fn executable(path: &Path) -> Result<(), String> {
    if !fs::symlink_metadata(path)
        .map_err(|_| "runtime or seed executable is missing")?
        .file_type()
        .is_file()
    {
        return Err("runtime or seed executable is not a regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let destination = target.join(entry.file_name());
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else if kind.is_file() {
            if destination.exists() {
                return Err("runtime/payload archives contain overlapping files".into());
            }
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        } else {
            return Err("runtime/payload trees must not contain symlinks or special files".into());
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tree {
    key: String,
    files: BTreeMap<String, (Sha256Digest, bool)>,
}

pub fn seal(path: &Path, key: &str) -> Result<(), String> {
    profile_store::write_json(
        &path.join(".isolock-tree.json"),
        &Tree {
            key: key.into(),
            files: files(path)?,
        },
        true,
    )
}

pub fn reusable(path: &Path, key: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let bytes = fs::read(path.join(".isolock-tree.json"))
        .map_err(|_| "existing derived tree has no verification receipt")?;
    let tree: Tree = serde_json::from_slice(&bytes).map_err(|_| "invalid derived tree receipt")?;
    if tree.key != key || tree.files != files(path)? {
        return Err(
            "existing derived tree was modified or belongs to a different derivation".into(),
        );
    }
    Ok(true)
}

pub(super) fn files(root: &Path) -> Result<BTreeMap<String, (Sha256Digest, bool)>, String> {
    let mut files = BTreeMap::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn visit(
    root: &Path,
    path: &Path,
    output: &mut BTreeMap<String, (Sha256Digest, bool)>,
) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "tree path escapes its root")?
            .to_str()
            .ok_or("tree path is not UTF-8")?
            .to_owned();
        if relative == ".isolock-tree.json" {
            continue;
        }
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_dir() {
            visit(root, &path, output)?;
        } else if kind.is_file() {
            output.insert(
                relative,
                (
                    digest_file(&path).map_err(|error| error.to_string())?,
                    has_execute_bit(&path)?,
                ),
            );
        } else {
            return Err("derived tree contains a link or special file".into());
        }
    }
    Ok(())
}

fn has_execute_bit(path: &Path) -> Result<bool, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions()
            .mode()
            & 0o111
            != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(false)
    }
}

pub fn publish(stage: tempfile::TempDir, destination: &Path) -> Result<PathBuf, String> {
    if destination.exists() {
        return Err("derived destination already exists".into());
    }
    fs::create_dir_all(
        destination
            .parent()
            .ok_or("derived destination has no parent")?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(stage.path(), destination).map_err(|error| error.to_string())?;
    Ok(destination.into())
}

/// Strip set-id/sticky and group/world write bits before exposing an archive tree.
fn normalize_permissions(root: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let directory = entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir();
            let mode = if directory || has_execute_bit(&path)? {
                0o755
            } else {
                0o644
            };
            fs::set_permissions(&path, fs::Permissions::from_mode(mode))
                .map_err(|error| error.to_string())?;
            if directory {
                normalize_permissions(&path)?;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = root;
    }
    Ok(())
}
