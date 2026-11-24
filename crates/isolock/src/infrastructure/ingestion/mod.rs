//! Explicit reviewed-byte import. Planning and restoration never acquire upstream bytes.

mod acquire;
mod epoch;
mod git;
mod manifest;
mod profiles;
mod publish;
mod retention;
mod roots;

pub use publish::{publish, Publication};
pub use roots::roots;

use crate::domain::Settings;
use crate::infrastructure::profile_store::{self, StoreLease, Workspace};
use manifest::Manifest;
use std::path::Path;

/// A reviewed NAME==VERSION selection within one ecosystem.
pub struct Selection<'a> {
    pub registry: Option<&'a str>,
    pub packages: &'a [String],
}

/// One reviewed import request.
pub struct Import<'a> {
    /// Project root every relative path resolves against.
    pub root: &'a Path,
    /// Validated settings describing the target CAS.
    pub settings: &'a Settings,
    /// Reviewed JSON pins; never resolves package versions.
    pub manifest: &'a Path,
    /// Directory holding the original archive basenames.
    pub artifacts: &'a Path,
    /// The reviewed ecosystem/package selection.
    pub selection: Selection<'a>,
    /// Whether the pinned HTTPS sources are acquired with curl.
    pub download: bool,
}

pub fn ingest(request: Import<'_>) -> Result<String, String> {
    let Import {
        root,
        settings,
        manifest,
        artifacts,
        selection,
        download,
    } = request;
    let manifest =
        Manifest::read(&root.join(manifest))?.select(selection.registry, selection.packages)?;
    // Verify the whole selected batch before changing the configured CAS.
    let staged = acquire::stage(&manifest, &root.join(artifacts), download)?;
    let workspace = Workspace::new(root, settings);
    let _lease = StoreLease::acquire(&workspace)?;
    // Preflight all cache hits as well: never repair an existing corrupt blob.
    for artifact in &manifest.artifacts {
        workspace
            .cas
            .verified(&artifact.sha256)
            .map_err(|error| error.to_string())?;
    }
    for artifact in &manifest.artifacts {
        workspace
            .cas
            .put_file(&artifact.sha256, &staged.store.path(&artifact.sha256))
            .map_err(|error| error.to_string())?;
    }
    profile_store::write_json(
        &root.join(format!(
            ".isolock/ingestion/objects/{}.json",
            manifest.digest()?.as_str()
        )),
        &manifest,
        true,
    )?;
    profile_store::refresh_roots(&workspace, &[])?;
    Ok(format!(
        "Verified {} reviewed artifact(s) in CAS; publication requires isolock push",
        manifest.artifacts.len()
    ))
}
