//! Adapts reviewed ingestion and publication to the [`Ingestion`] port.

use crate::application::ports::{
    Ingestion, Publication, Selection, UseCaseResult, WorkspaceSelection,
};
use crate::domain::{DomainError, Settings};
use crate::infrastructure::ingestion;
use std::path::{Path, PathBuf};

/// Reviewed ingestion and publication bound to one project root and its settings.
pub struct ReviewedIngestion {
    root: PathBuf,
    settings: Settings,
    workspace: WorkspaceSelection,
}

impl ReviewedIngestion {
    /// Binds ingestion to one project root, its settings, and the explicit selection.
    #[must_use]
    pub fn new(root: &Path, settings: &Settings, workspace: WorkspaceSelection) -> Self {
        Self {
            root: root.into(),
            settings: settings.clone(),
            workspace,
        }
    }
}

impl Ingestion for ReviewedIngestion {
    fn ingest(
        &self,
        manifest: &Path,
        artifacts: &Path,
        selection: Selection<'_>,
        download: bool,
    ) -> UseCaseResult<String> {
        ingestion::ingest(ingestion::Import {
            root: &self.root,
            settings: &self.settings,
            manifest,
            artifacts,
            selection: ingestion::Selection {
                registry: selection.registry,
                packages: selection.packages,
            },
            download,
        })
        .map_err(DomainError::new)
    }

    fn publish(&self, request: Publication<'_>) -> UseCaseResult<String> {
        ingestion::publish(ingestion::Publication {
            root: &self.root,
            settings: &self.settings,
            workspace: &self.workspace,
            snapshot: request.snapshot,
            path: request.manifest,
            remote: request.remote,
            epoch_policy: request.epoch_policy,
            selection: ingestion::Selection {
                registry: request.selection.registry,
                packages: request.selection.packages,
            },
        })
        .map_err(DomainError::new)
    }
}
