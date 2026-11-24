//! Adapters that implement application ports with real files, Git, HTTP, and processes.

pub mod config;
pub mod execution;
pub mod ingestion;
pub mod ingestion_adapter;
pub mod legacy_plan;
pub mod materialize;
pub mod materializer_adapter;
pub mod plan_store;
pub mod process;
#[cfg(test)]
mod process_tests;
pub mod profile_store;
pub mod proxy_runtime;
pub mod transport;
pub mod transport_adapter;

pub use config::FilesystemConfiguration;
pub use ingestion_adapter::ReviewedIngestion;
pub use legacy_plan::WorkspacePlan;
pub use materializer_adapter::FileMaterializer;
pub use plan_store::FilePlanStore;
pub use proxy_runtime::LoopbackProxy;
pub use transport_adapter::FileTransport;
