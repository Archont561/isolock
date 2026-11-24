//! Use cases: one function per user-visible workflow, expressed in ports and domain types.
//!
//! Nothing here imports `clifacet`, reads process arguments, or prints. Each use case returns
//! a typed view that `cli::view` renders, so the observable output stays adapter-owned.

pub mod bootstrap;
pub mod configuration;
pub mod execution;
pub mod plan;

pub use bootstrap::{
    export_profile, fetch_profile, garbage_collect, seed_manifest, sync_profile, FetchOutcome,
    SeedOutcome, SyncOutcome,
};
pub use configuration::{
    check_configuration, explain_configuration, show_configuration, ConfigurationOutcome,
};
pub use execution::{exec_child, serve_proxy, ExecOutcome};
pub use plan::{check_airlock_plan, check_locked_profile, plan_profile, PlanOutcome};

#[cfg(test)]
mod tests;
