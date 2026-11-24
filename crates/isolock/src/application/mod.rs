//! The application layer: use cases and the ports they depend on.
//!
//! Dependency rule: `domain <- application <- {infrastructure, cli}`. Nothing in this module
//! imports `clifacet`, reads `std::env::args`, prints to a stream, or names a concrete adapter.

pub mod ports;
pub mod use_cases;

pub use ports::UseCaseResult;
