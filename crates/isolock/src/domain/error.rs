//! Typed failures the domain uses instead of formatted strings.
//!
//! The CLI adapter maps every variant to `clifacet::Failure::execution`, so the observable
//! diagnostics stay identical to the string-typed era while the layers above stop
//! matching on prose.

use std::fmt;

/// A rejected input or violated invariant, described for a person reading a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainError {
    message: String,
}

impl DomainError {
    /// Records a rejected value or violated invariant.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The caller-facing message, without any adapter prefix.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Appends a contextual suffix, keeping a single message for the operator.
    #[must_use]
    pub fn with(self, message: impl Into<String>) -> Self {
        Self::new(format!("{}: {}", self.message, message.into()))
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DomainError {}

impl From<String> for DomainError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<DomainError> for String {
    fn from(error: DomainError) -> Self {
        error.message
    }
}
