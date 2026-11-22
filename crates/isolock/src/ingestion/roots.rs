//! Reviewed originals remain GC roots, including before their explicit publication.

use super::manifest::Manifest;
use isolock_cas::Sha256Digest;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub fn roots(root: &Path) -> Result<(Vec<String>, BTreeSet<String>), String> {
    let directory = root.join(".isolock/ingestion/objects");
    let mut names = Vec::new();
    let mut blobs = BTreeSet::new();
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((names, blobs)),
        Err(_) => return Err("cannot inspect ingestion GC roots".into()),
    };
    if !metadata.file_type().is_dir() {
        return Err("ingestion GC roots must be a regular directory".into());
    }
    for entry in fs::read_dir(directory).map_err(|_| "cannot enumerate ingestion GC roots")? {
        let entry = entry.map_err(|_| "cannot inspect ingestion GC root")?;
        if !entry
            .file_type()
            .map_err(|_| "cannot inspect ingestion GC root")?
            .is_file()
        {
            return Err("ingestion GC root must be a regular JSON object".into());
        }
        let name = entry.file_name();
        let digest = name
            .to_str()
            .and_then(|name| name.strip_suffix(".json"))
            .and_then(|name| Sha256Digest::parse(name).ok())
            .ok_or("invalid ingestion GC root filename")?;
        if name != format!("{}.json", digest.as_str()).as_str() {
            return Err("noncanonical ingestion GC root filename".into());
        }
        let manifest = Manifest::read(&entry.path())?;
        if manifest.digest()? != digest {
            return Err("ingestion GC root object digest mismatch".into());
        }
        names.push(format!("ingestion/{}", digest.as_str()));
        blobs.extend(
            manifest
                .artifacts
                .into_iter()
                .map(|artifact| artifact.sha256.as_str().to_owned()),
        );
    }
    names.sort();
    Ok((names, blobs))
}
