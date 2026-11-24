//! Layered Isolock: domain policy, application use cases and ports, adapters, CLI surface.
//!
//! The layers are internal to the binary. Each one is exercised directly by tests in its own
//! module, which is what proves use cases run without clifacet: `cli` is the only module that
//! names it, and `main` only calls [`run`].

mod application;
mod cli;
mod domain;
mod infrastructure;

pub use cli::run;
