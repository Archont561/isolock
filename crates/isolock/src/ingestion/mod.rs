//! Explicit reviewed-byte import. Planning and restoration never acquire upstream bytes.

mod acquire;
mod git;
mod manifest;
mod profiles;
mod publish;
mod roots;

pub use publish::publish;
pub use roots::roots;

use crate::config::{self, Overrides};
use crate::profile_store::{self, StoreLease, Workspace};
use manifest::Manifest;
use std::path::Path;

pub struct Selection<'a> {
    pub registry: Option<&'a str>,
    pub packages: &'a [String],
}

pub fn ingest(
    overrides: &Overrides,
    manifest: &Path,
    artifacts: &Path,
    selection: Selection<'_>,
    download: bool,
) -> Result<String, String> {
    let root = std::env::current_dir().map_err(|_| "cannot resolve project directory")?;
    let settings = config::load(&root, overrides)?.settings;
    let manifest =
        Manifest::read(&root.join(manifest))?.select(selection.registry, selection.packages)?;
    // Verify the whole selected batch before changing the configured CAS.
    let staged = acquire::stage(&manifest, &root.join(artifacts), download)?;
    let workspace = Workspace::new(&root, &settings);
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
