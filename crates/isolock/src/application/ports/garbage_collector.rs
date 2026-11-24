//! Conservative collection of unreachable CAS blobs.

use super::UseCaseResult;

/// What one collection pass found, in the order the operator reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectionReport {
    /// Ingestion roots that keep blobs alive.
    pub roots: Vec<String>,
    /// Digests referenced by any root.
    pub retained: Vec<String>,
    /// Digests no root references.
    pub reclaimable: Vec<String>,
    /// Bytes the reclaimable digests occupy.
    pub bytes: u64,
    /// Whether the pass deleted anything.
    pub applied: bool,
}

impl CollectionReport {
    /// Renders the report exactly as the reviewed JSON document.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the report cannot be serialized.
    pub fn render(&self) -> UseCaseResult<String> {
        let document = serde_json::json!({
            "roots": self.roots,
            "retained": self.retained,
            "reclaimable": self.reclaimable,
            "bytes": self.bytes,
            "applied": self.applied,
        });
        serde_json::to_string_pretty(&document)
            .map_err(|error| crate::domain::DomainError::new(error.to_string()))
    }
}

/// Collects unreachable blobs after re-reading every registered root.
pub trait GarbageCollector {
    /// Runs one pass, deleting only when `apply` is set.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when no root exists or a registration is unsafe.
    fn collect(&self, roots: &[String], apply: bool) -> UseCaseResult<CollectionReport>;
}
