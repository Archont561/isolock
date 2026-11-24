//! The legacy airlock plan, as a port so `plan` can diff it without reading JSON itself.

use super::UseCaseResult;

/// Whether the locked legacy plan's artifacts exist in CAS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftState {
    /// Every reviewed artifact is present and hashes correctly.
    InSync,
    /// The plan is not satisfied; the vectors are operator-facing labels.
    Drift {
        /// `name==version (registry)` labels for artifacts missing from CAS.
        missing: Vec<String>,
        /// Digests whose stored bytes no longer match the plan.
        mismatched_digests: Vec<String>,
        /// Ready-to-run dispatch commands for the missing artifacts.
        commands: Vec<String>,
    },
}

/// Reads the legacy `plan.json` and diffs it against local CAS bytes.
pub trait LegacyPlan {
    /// Whether a usable legacy plan exists on disk.
    fn exists(&self) -> bool;

    /// The plan path reported when no plan exists.
    fn path(&self) -> String;

    /// Compares the locked plan against local CAS.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the plan or CAS is unreadable, or a
    /// dispatch command cannot be rendered.
    fn drift(&self) -> UseCaseResult<DriftState>;
}
