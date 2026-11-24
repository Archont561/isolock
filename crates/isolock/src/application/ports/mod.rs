//! Ports are the interfaces use cases depend on; adapters live in `infrastructure` and `cli`.
//!
//! A use case never names a concrete adapter. Tests supply fakes, and the composition root
//! supplies the reviewed implementations, without the use case changing.

mod config_provider;
mod garbage_collector;
mod ingestion;
mod legacy_plan;
mod materializer;
mod profile_store;
mod progress;
mod proxy_runtime;
mod transport;

pub use config_provider::{
    ConfigProvenance, ConfigurationProvider, LayeredOverrides, LoadedConfiguration,
    SessionSettings, WorkspaceSelection,
};
pub use garbage_collector::{CollectionReport, GarbageCollector};
pub use ingestion::{Ingestion, ProfileSnapshot, Publication, Selection};
pub use legacy_plan::{DriftState, LegacyPlan};
pub use materializer::Materializer;
pub use profile_store::{LockGuard, PlanStore, ProfileGraph};
pub use progress::Progress;
#[cfg(test)]
pub use progress::Recorded;
pub use proxy_runtime::{ChildCommand, Listener, ProxyRuntime, Session as ProxySession};
pub use transport::{ArtifactTransport, BlobStore, SourceSnapshot};

use crate::domain::DomainError;

/// The result type use cases speak in, so the CLI adapter owns message formatting.
pub type UseCaseResult<T> = Result<T, DomainError>;
