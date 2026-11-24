//! Adapts profile materialization to the [`Materializer`] port.

use crate::application::ports::{Materializer, ProfileGraph, UseCaseResult};
use crate::domain::DomainError;
use crate::infrastructure::materialize;
use crate::infrastructure::profile_store::{self, Workspace};
use isolock_plan::profile::ProfileManifest;
use std::path::{Path, PathBuf};

/// Materializes reviewed graphs into `.isolock` prefixes on this host.
pub struct FileMaterializer {
    workspace: Workspace,
}

impl FileMaterializer {
    /// Binds materialization to the store of one project.
    #[must_use]
    pub fn new(workspace: Workspace) -> Self {
        Self { workspace }
    }

    /// The default seed manifest path for one profile and platform.
    #[must_use]
    pub fn seed_path(&self, profile: &str, platform: &str) -> PathBuf {
        self.workspace
            .root
            .join(".isolock/seeds")
            .join(format!("{profile}-{platform}.manifest"))
    }
}

impl Materializer for FileMaterializer {
    fn install(&self, manifests: &ProfileGraph, selected: &str) -> UseCaseResult<PathBuf> {
        materialize::install(&self.workspace, manifests, selected).map_err(DomainError::new)
    }

    fn bootstrap_manifest(&self, manifest: &ProfileManifest) -> UseCaseResult<String> {
        materialize::bootstrap_manifest(manifest).map_err(DomainError::new)
    }

    fn write_seed(&self, manifest_path: &Path, contents: &str) -> UseCaseResult<()> {
        profile_store::write_file(manifest_path, contents.as_bytes(), false)
            .map_err(DomainError::new)?;
        profile_store::write_file(
            &self.workspace.root.join(".isolock/bootstrap.sh"),
            include_bytes!("../../../../templates/bootstrap.sh"),
            false,
        )
        .map_err(DomainError::new)
    }

    fn seed_path(&self, profile: &str, platform: &str) -> UseCaseResult<PathBuf> {
        Ok(FileMaterializer::seed_path(self, profile, platform))
    }

    fn host_platform(&self) -> Option<String> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Some("linux-64".into()),
            ("linux", "aarch64") => Some("linux-aarch64".into()),
            ("macos", "x86_64") => Some("osx-64".into()),
            ("macos", "aarch64") => Some("osx-arm64".into()),
            _ => None,
        }
    }
}
