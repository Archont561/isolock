//! The composition root: every concrete adapter is constructed here and nowhere else.
//!
//! clifacet builds this once, lazily, after validation and before the command runs, so help,
//! version, and usage errors never construct a workspace or bind a listener.

use crate::application::ports::{
    ConfigurationProvider, LayeredOverrides, SessionSettings, WorkspaceSelection,
};
use crate::application::use_cases::plan;
use crate::cli::error;
use crate::domain::Settings;
use crate::infrastructure::{
    config, execution, profile_store, FileMaterializer, FilePlanStore, FileTransport,
    FilesystemConfiguration, LoopbackProxy, ReviewedIngestion, WorkspacePlan,
};
use clifacet::Failure;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;

/// Everything a command handler may use, already wired to reviewed adapters.
#[derive(Clone, Debug)]
pub struct AppContext {
    root: PathBuf,
    settings: Settings,
    selection: WorkspaceSelection,
    overrides: LayeredOverrides,
    child_status: Rc<Cell<Option<ExitCode>>>,
}

impl AppContext {
    /// Builds the context from the parsed root controller.
    ///
    /// # Errors
    /// Returns a [`Failure`] when the project directory or configuration is invalid.
    pub fn from_cli(controller: &crate::cli::Isolock) -> Result<Self, Failure> {
        Self::new(
            LayeredOverrides::from(controller.overrides()),
            Rc::new(Cell::new(None)),
        )
    }

    /// Resolves configuration once and projects it into the collaborators below.
    ///
    /// # Errors
    /// Returns a [`Failure`] when the project directory or configuration is invalid.
    pub fn new(
        overrides: LayeredOverrides,
        child_status: Rc<Cell<Option<ExitCode>>>,
    ) -> Result<Self, Failure> {
        let root = std::env::current_dir()
            .map_err(|_| Failure::execution("cannot resolve project directory"))?;
        let selection = WorkspaceSelection {
            profile: overrides.profile.clone(),
            platform: overrides.platform.clone(),
        };
        let provider = FilesystemConfiguration::new(overrides.clone());
        let loaded = provider.load(&root).map_err(error::execution)?;
        Ok(Self {
            root,
            settings: loaded.settings,
            selection,
            overrides,
            child_status,
        })
    }

    /// The project directory the command operates on.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The validated settings.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The explicit profile/platform selection.
    #[must_use]
    pub fn selection(&self) -> &WorkspaceSelection {
        &self.selection
    }

    /// Records the status a child process owns.
    pub fn record_child_status(&self, status: ExitCode) {
        self.child_status.set(Some(status));
    }

    /// Copies the recorded child status, if a child ran to completion.
    #[must_use]
    pub fn taken_child_status(&self) -> Option<ExitCode> {
        self.child_status.get()
    }

    /// The layered configuration provider, for `config` commands.
    #[must_use]
    pub fn configuration(&self) -> FilesystemConfiguration {
        FilesystemConfiguration::new(self.overrides.clone())
    }

    /// The reviewed plan store.
    #[must_use]
    pub fn store(&self) -> FilePlanStore {
        FilePlanStore::new(&self.root, &self.settings)
    }

    /// The configured artifact transport.
    #[must_use]
    pub fn transport(&self) -> FileTransport {
        FileTransport::new(&self.root, &self.settings)
    }

    /// The profile materializer.
    #[must_use]
    pub fn materializer(&self) -> FileMaterializer {
        FileMaterializer::new(profile_store::Workspace::new(&self.root, &self.settings))
    }

    /// The proxy runtime projection.
    ///
    /// # Errors
    /// Returns a [`Failure`] when configuration cannot be re-projected.
    pub fn runtime(&self) -> Result<LoopbackProxy, Failure> {
        self.session().map(LoopbackProxy::new)
    }

    /// The resolved execution session projection.
    ///
    /// When an explicit profile is selected the session runs inside that profile; with no
    /// selection the legacy airlock-plan session is returned untouched.
    ///
    /// # Errors
    /// Returns a [`Failure`] when configuration cannot be re-projected, the locked manifest
    /// is missing or drifted, or the profile has not been materialized yet.
    pub fn session(&self) -> Result<SessionSettings, Failure> {
        let provider = FilesystemConfiguration::new(self.overrides.clone());
        let loaded = provider.load(&self.root).map_err(error::execution)?;
        let settings = config::session(&self.root, &loaded).map_err(error::execution)?;
        let (Some(profile), Some(platform)) = (
            self.selection.profile.as_deref(),
            self.selection.platform.as_deref(),
        ) else {
            return Ok(settings);
        };
        let store = self.store();
        let graph = plan::locked_graph(&store, &self.settings, profile, platform)
            .map_err(error::execution)?;
        let manifest = graph.get(profile).ok_or_else(|| {
            error::execution(crate::domain::DomainError::new(
                "selected profile is missing",
            ))
        })?;
        execution::profile_session::prepare(&settings, manifest, &self.root)
            .map_err(|message| error::execution(crate::domain::DomainError::new(message)))
    }

    /// The legacy airlock plan adapter.
    ///
    /// # Errors
    /// Returns a [`Failure`] when configuration cannot be re-projected.
    pub fn legacy_plan(&self) -> Result<WorkspacePlan, Failure> {
        self.session().map(WorkspacePlan::new)
    }

    /// The reviewed ingestion adapter.
    #[must_use]
    pub fn ingestion(&self) -> ReviewedIngestion {
        ReviewedIngestion::new(&self.root, &self.settings, self.selection.clone())
    }
}

impl From<&AppContext> for AppContext {
    fn from(context: &AppContext) -> Self {
        context.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_of_the_context_shares_the_child_status_cell() {
        let context = AppContext::new(LayeredOverrides::new(), Rc::new(Cell::new(None)))
            .expect("default configuration resolves");
        let copy = AppContext::from(&context);
        copy.record_child_status(ExitCode::from(3));
        assert_eq!(context.taken_child_status(), Some(ExitCode::from(3)));
    }
}
